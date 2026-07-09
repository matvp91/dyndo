use std::process::Command;

#[test]
fn writes_asset_json_for_video_and_audio() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("asset.json");
    let fixtures = concat!(env!("CARGO_MANIFEST_DIR"), "/../dyndo-core/tests/fixtures");

    let status = Command::new(env!("CARGO_BIN_EXE_dyndo"))
        .arg("index")
        .arg("-i")
        .arg(format!("{fixtures}/video_avc_1080.mp4"))
        .arg("-i")
        .arg(format!("{fixtures}/audio_aac_nl_2.mp4"))
        .arg("-o")
        .arg(&out)
        .status()
        .unwrap();
    assert!(status.success());

    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap();
    assert_eq!(json["tracks"].as_array().unwrap().len(), 2);
    assert_eq!(json["tracks"][0]["type"], "video");
    assert_eq!(json["tracks"][0]["fourcc"], "avc1");
    assert_eq!(json["tracks"][1]["type"], "audio");
    assert_eq!(json["tracks"][1]["language"], "nld");
}

#[test]
fn generates_mpd_from_asset_json() {
    let dir = tempfile::tempdir().unwrap();
    let asset = dir.path().join("asset.json");
    let mpd = dir.path().join("stream.mpd");
    let fixtures = concat!(env!("CARGO_MANIFEST_DIR"), "/../dyndo-core/tests/fixtures");

    let index = Command::new(env!("CARGO_BIN_EXE_dyndo"))
        .arg("index")
        .arg("-i")
        .arg(format!("{fixtures}/video_avc_1080.mp4"))
        .arg("-i")
        .arg(format!("{fixtures}/audio_aac_nl_2.mp4"))
        .arg("-o")
        .arg(&asset)
        .status()
        .unwrap();
    assert!(index.success());

    let dash = Command::new(env!("CARGO_BIN_EXE_dyndo"))
        .arg("dash")
        .arg("-i")
        .arg(&asset)
        .arg("-o")
        .arg(&mpd)
        .status()
        .unwrap();
    assert!(dash.success());

    let xml = std::fs::read_to_string(&mpd).unwrap();
    assert!(xml.contains("type=\"static\""));
    assert!(xml.contains("<SegmentTimeline>"));
    assert!(xml.contains("codecs=\"avc1.640028\""));
    assert!(xml.contains("codecs=\"mp4a.40.2\""));
}
