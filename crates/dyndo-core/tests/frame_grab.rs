use dyndo_core::image::FrameExtractor;
use dyndo_core::track::Track;
use image::{GenericImageView, ImageFormat, RgbImage};
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
    let track = Track::probe(&operator, path, None).await.unwrap();

    let jpeg = FrameExtractor::new(&operator, track.native_cmaf().unwrap())
        .jpeg(0, 16, 16)
        .await
        .unwrap();
    let image = image::load_from_memory_with_format(&jpeg, ImageFormat::Jpeg)
        .unwrap()
        .to_rgb8();

    assert_eq!(image.dimensions(), (16, 16));
    assert!(is_nearly_black(&image));
}

#[tokio::test]
async fn jpeg_returns_the_requested_dimensions() {
    let operator = memory_operator();
    let path = RelativePath::new("video.mp4");
    operator.write(path.as_str(), VIDEO_FIXTURE).await.unwrap();
    let track = Track::probe(&operator, path, None).await.unwrap();

    let jpeg = FrameExtractor::new(&operator, track.native_cmaf().unwrap())
        .jpeg(0, 8, 4)
        .await
        .unwrap();
    let image = image::load_from_memory_with_format(&jpeg, ImageFormat::Jpeg).unwrap();

    assert_eq!(image.dimensions(), (8, 4));
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
    let track = Track::probe(&operator, path, None).await.unwrap();
    let extractor = FrameExtractor::new(&operator, track.native_cmaf().unwrap());

    let before_boundary = jpeg_image(&extractor, 499).await;
    let at_boundary = jpeg_image(&extractor, 500).await;
    let after_boundary = jpeg_image(&extractor, 999).await;

    assert_eq!(track.native_cmaf().unwrap().segments().len(), 2);
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
    let track = Track::probe(&operator, path, None).await.unwrap();
    let error = FrameExtractor::new(&operator, track.native_cmaf().unwrap())
        .jpeg(1_000, 16, 16)
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
    let track = Track::probe(&operator, path, None).await.unwrap();
    let extractor = FrameExtractor::new(&operator, track.native_cmaf().unwrap());
    let image = jpeg_image(&extractor, 500).await;

    assert!(is_predominantly_blue(&image));
}

async fn jpeg_image(extractor: &FrameExtractor<'_>, time: u64) -> RgbImage {
    let jpeg = extractor.jpeg(time, 16, 16).await.unwrap();
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
