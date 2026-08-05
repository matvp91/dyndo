use std::fs;
use std::path::Path;
use std::process::Command;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures");

fn stage(dir: &Path, fixtures: &[&str]) {
    for fixture in fixtures {
        fs::copy(format!("{FIXTURES}/{fixture}"), dir.join(fixture)).unwrap();
    }
}

fn dyndo(dir: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_dyndo"));
    command.env("OPENDAL_FS_ROOT", dir);
    command
}

fn index_video_and_audio(dir: &Path) {
    stage(dir, &["video_avc_1080.mp4", "audio_aac_nl_2.mp4"]);
    let status = dyndo(dir)
        .args([
            "index",
            "video_avc_1080.mp4",
            "audio_aac_nl_2.mp4",
            "-o",
            "asset.json",
        ])
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn index_writes_asset_descriptor() {
    let dir = tempfile::tempdir().unwrap();
    index_video_and_audio(dir.path());

    let json: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("asset.json")).unwrap()).unwrap();
    let tracks = json["tracks"].as_array().unwrap();

    assert_eq!(tracks.len(), 2);
    assert!(tracks[0]["id"].as_str().unwrap().starts_with("video_"));
    assert!(tracks[1]["id"].as_str().unwrap().starts_with("audio_"));
    assert_eq!(tracks[0]["codec"], "avc1.640028");
    assert_eq!(tracks[1]["codec"], "mp4a.40.2");
}

#[test]
fn dash_writes_manifest() {
    let dir = tempfile::tempdir().unwrap();
    index_video_and_audio(dir.path());

    let status = dyndo(dir.path())
        .args(["dash", "-i", "asset.json", "-o", "stream.mpd"])
        .status()
        .unwrap();
    assert!(status.success());

    let xml = fs::read_to_string(dir.path().join("stream.mpd")).unwrap();
    assert!(xml.contains("type=\"static\"") && xml.contains("<SegmentTimeline>"));
}

#[test]
fn hls_writes_master_and_track_playlists() {
    let dir = tempfile::tempdir().unwrap();
    index_video_and_audio(dir.path());
    let descriptor: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("asset.json")).unwrap()).unwrap();

    let status = dyndo(dir.path())
        .args(["hls", "-i", "asset.json", "-o", "playlists"])
        .status()
        .unwrap();
    assert!(status.success());

    assert!(dir.path().join("playlists/master.m3u8").is_file());
    for track in descriptor["tracks"].as_array().unwrap() {
        let id = track["id"].as_str().unwrap();
        assert!(dir.path().join(format!("playlists/{id}.m3u8")).is_file());
    }
}

#[test]
fn index_rejects_unknown_track_option() {
    let dir = tempfile::tempdir().unwrap();
    stage(dir.path(), &["audio_aac_nl_2.mp4"]);

    let status = dyndo(dir.path())
        .args(["index", "audio_aac_nl_2.mp4,codec=aac", "-o", "asset.json"])
        .status()
        .unwrap();

    assert!(!status.success());
}
