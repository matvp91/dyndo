use dyndo_core::image::FrameGrab;
use dyndo_core::segment_options::SegmentOptions;
use dyndo_core::track::Track;
use image::{ImageFormat, RgbImage};
use opendal::{Operator, services::Memory};
use relative_path::RelativePath;

const VIDEO_FIXTURE: &[u8] = include_bytes!("fixtures/three-frame-black-h264.mp4");
const TWO_SEGMENT_VIDEO_FIXTURE: &[u8] =
    include_bytes!("fixtures/two-segment-black-white-h264.mp4");
const INTERFRAME_VIDEO_FIXTURE: &[u8] = include_bytes!("fixtures/four-colour-interframe-h264.mp4");

fn memory_operator() -> Operator {
    Operator::new(Memory::default()).unwrap()
}

#[tokio::test]
async fn jpeg_decodes_a_black_frame_at_the_requested_time() {
    let operator = memory_operator();
    let path = RelativePath::new("video.mp4");
    operator.write(path.as_str(), VIDEO_FIXTURE).await.unwrap();
    let track = Track::probe(&operator, path, None, &SegmentOptions::default())
        .await
        .unwrap();

    let jpeg = FrameGrab::new(&operator, &track)
        .unwrap()
        .jpeg(0)
        .await
        .unwrap();
    let image = image::load_from_memory_with_format(&jpeg, ImageFormat::Jpeg)
        .unwrap()
        .to_rgb8();

    assert_eq!(image.dimensions(), (16, 16));
    assert!(is_nearly_black(&image));
}

fn is_nearly_black(image: &RgbImage) -> bool {
    image
        .pixels()
        .all(|pixel| pixel.0.iter().all(|&channel| channel <= 5))
}

#[tokio::test]
async fn jpeg_selects_the_frame_on_each_side_of_a_media_segment_boundary() {
    let operator = memory_operator();
    let path = RelativePath::new("video.mp4");
    operator
        .write(path.as_str(), TWO_SEGMENT_VIDEO_FIXTURE)
        .await
        .unwrap();
    let track = Track::probe(&operator, path, None, &SegmentOptions::default())
        .await
        .unwrap();
    let grab = FrameGrab::new(&operator, &track).unwrap();

    let before_boundary = jpeg_image(&grab, 499).await;
    let at_boundary = jpeg_image(&grab, 500).await;
    let after_boundary = jpeg_image(&grab, 999).await;

    assert_eq!(track.segments().len(), 2);
    assert!(is_nearly_black(&before_boundary));
    assert!(is_nearly_white(&at_boundary));
    assert!(is_nearly_white(&after_boundary));
}

#[tokio::test]
async fn jpeg_rejects_a_time_at_the_end_of_the_video_track() {
    let operator = memory_operator();
    let path = RelativePath::new("video.mp4");
    operator
        .write(path.as_str(), TWO_SEGMENT_VIDEO_FIXTURE)
        .await
        .unwrap();
    let track = Track::probe(&operator, path, None, &SegmentOptions::default())
        .await
        .unwrap();
    let error = FrameGrab::new(&operator, &track)
        .unwrap()
        .jpeg(1_000)
        .await
        .unwrap_err();

    assert_eq!(error.to_string(), "time 1000 ms is outside the video track");
}

#[tokio::test]
async fn jpeg_seeks_from_a_keyframe_to_the_requested_interframe() {
    let operator = memory_operator();
    let path = RelativePath::new("video.mp4");
    operator
        .write(path.as_str(), INTERFRAME_VIDEO_FIXTURE)
        .await
        .unwrap();
    let track = Track::probe(&operator, path, None, &SegmentOptions::default())
        .await
        .unwrap();
    let grab = FrameGrab::new(&operator, &track).unwrap();
    let image = jpeg_image(&grab, 500).await;

    assert!(is_predominantly_blue(&image));
}

async fn jpeg_image(grab: &FrameGrab<'_>, time: u64) -> RgbImage {
    let jpeg = grab.jpeg(time).await.unwrap();
    image::load_from_memory_with_format(&jpeg, ImageFormat::Jpeg)
        .unwrap()
        .to_rgb8()
}

fn is_nearly_white(image: &RgbImage) -> bool {
    image
        .pixels()
        .all(|pixel| pixel.0.iter().all(|&channel| channel >= 250))
}

fn is_predominantly_blue(image: &RgbImage) -> bool {
    image.pixels().all(|pixel| {
        u16::from(pixel.0[2]) > u16::from(pixel.0[0]) + 40
            && u16::from(pixel.0[2]) > u16::from(pixel.0[1]) + 40
    })
}
