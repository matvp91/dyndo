use dyndo_core::image::FrameGrab;
use dyndo_core::segment_options::SegmentOptions;
use dyndo_core::track::Track;
use image::{ImageFormat, RgbImage};
use opendal::{Operator, services::Memory};
use relative_path::RelativePath;

const VIDEO_FIXTURE: &[u8] = include_bytes!("fixtures/three-frame-black-h264.mp4");

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
