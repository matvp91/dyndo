use std::fs;
use std::path::Path;
use std::process::Command;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures");

/// Copy `fixtures` into `dir` so they're addressable under `OPENDAL_FS_ROOT=dir`.
fn stage(dir: &Path, fixtures: &[&str]) {
    for f in fixtures {
        fs::copy(format!("{FIXTURES}/{f}"), dir.join(f)).unwrap();
    }
}

/// A `dyndo` command whose operator is rooted at `dir`.
fn dyndo(dir: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_dyndo"));
    cmd.env("OPENDAL_FS_ROOT", dir);
    cmd
}

#[test]
fn writes_asset_json_for_video_and_audio() {
    let dir = tempfile::tempdir().unwrap();
    stage(dir.path(), &["video_avc_1080.mp4", "audio_aac_nl_2.mp4"]);

    let status = dyndo(dir.path())
        .args([
            "index",
            "-i",
            "video_avc_1080.mp4",
            "-i",
            "audio_aac_nl_2.mp4",
            "-o",
            "asset.json",
        ])
        .status()
        .unwrap();
    assert!(status.success());

    let json: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("asset.json")).unwrap()).unwrap();
    let tracks = json["tracks"].as_array().unwrap();
    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[0]["type"], "video");
    assert_eq!(tracks[0]["fourcc"], "avc1");
    assert_eq!(tracks[0]["path"], "video_avc_1080.mp4");
    assert_eq!(tracks[1]["type"], "audio");
    assert_eq!(tracks[1]["language"], "nld");
}

#[test]
fn generates_mpd_from_asset_json() {
    let dir = tempfile::tempdir().unwrap();
    stage(dir.path(), &["video_avc_1080.mp4", "audio_aac_nl_2.mp4"]);

    assert!(dyndo(dir.path())
        .args([
            "index",
            "-i",
            "video_avc_1080.mp4",
            "-i",
            "audio_aac_nl_2.mp4",
            "-o",
            "asset.json",
        ])
        .status()
        .unwrap()
        .success());

    assert!(dyndo(dir.path())
        .args(["dash", "-i", "asset.json", "-o", "stream.mpd"])
        .status()
        .unwrap()
        .success());

    let xml = fs::read_to_string(dir.path().join("stream.mpd")).unwrap();
    assert!(xml.contains("type=\"static\""));
    assert!(xml.contains("<SegmentTimeline>"));
    assert!(xml.contains("codecs=\"avc1.640028\""));
    assert!(xml.contains("codecs=\"mp4a.40.2\""));
}

#[test]
fn dash_compact_flag_hoists_segment_template() {
    let dir = tempfile::tempdir().unwrap();
    stage(dir.path(), &["video_avc_1080.mp4", "audio_aac_nl_2.mp4"]);

    assert!(dyndo(dir.path())
        .args([
            "index",
            "-i",
            "video_avc_1080.mp4",
            "-i",
            "audio_aac_nl_2.mp4",
            "-o",
            "asset.json",
        ])
        .status()
        .unwrap()
        .success());

    assert!(dyndo(dir.path())
        .args(["dash", "-i", "asset.json", "-o", "plain.mpd"])
        .status()
        .unwrap()
        .success());

    assert!(dyndo(dir.path())
        .args(["dash", "-i", "asset.json", "-o", "compact.mpd", "-c"])
        .status()
        .unwrap()
        .success());

    let plain = fs::read_to_string(dir.path().join("plain.mpd")).unwrap();
    let compact = fs::read_to_string(dir.path().join("compact.mpd")).unwrap();
    // Compaction hoists each set's SegmentTemplate above its Representations,
    // changing the structure.
    assert_ne!(compact, plain);
    assert!(compact.contains("$RepresentationID$/$Time$.m4s"));
    // In compact output, the first SegmentTemplate precedes the first Representation.
    let first_rep = compact.find("<Representation").unwrap();
    let first_st = compact.find("<SegmentTemplate").unwrap();
    assert!(first_st < first_rep);
}
