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
        .args(["dash", "-i", "asset.json", "-o", "stream.mpd", "--compact"])
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
        .args([
            "hls",
            "-i",
            "asset.json",
            "-o",
            "playlists",
            "--segment-min-length",
            "10000",
        ])
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
fn dash_filter_omits_the_tracks_it_rejects() {
    let dir = tempfile::tempdir().unwrap();
    index_video_and_audio(dir.path());

    let status = dyndo(dir.path())
        .args(["dash", "-i", "asset.json", "--filter", "type!=audio"])
        .status()
        .unwrap();
    assert!(status.success());

    let xml = fs::read_to_string(dir.path().join("stream.mpd")).unwrap();
    assert!(
        xml.contains("contentType=\"video\"") && !xml.contains("contentType=\"audio\""),
        "unexpected manifest: {xml}"
    );
}

/// A media playlist is written per surviving track, so the filter has to narrow the
/// descriptor the loop walks — otherwise it writes playlists the multivariant
/// playlist does not reference.
#[test]
fn hls_filter_writes_playlists_only_for_the_tracks_it_keeps() {
    let dir = tempfile::tempdir().unwrap();
    index_video_and_audio(dir.path());
    let descriptor: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("asset.json")).unwrap()).unwrap();
    let id = |kind: &str| {
        descriptor["tracks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|track| track["type"] == kind)
            .map(|track| track["id"].as_str().unwrap().to_string())
            .unwrap()
    };

    let status = dyndo(dir.path())
        .args([
            "hls",
            "-i",
            "asset.json",
            "-o",
            "playlists",
            "--filter",
            "type!=audio",
        ])
        .status()
        .unwrap();
    assert!(status.success());

    let playlists = dir.path().join("playlists");
    assert!(playlists.join(format!("{}.m3u8", id("video"))).is_file());
    assert!(!playlists.join(format!("{}.m3u8", id("audio"))).is_file());
}

#[test]
fn a_malformed_filter_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    index_video_and_audio(dir.path());

    let output = dyndo(dir.path())
        .args(["dash", "-i", "asset.json", "--filter", "heigth<=720"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("invalid --filter"),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn a_filter_matching_nothing_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    index_video_and_audio(dir.path());

    let output = dyndo(dir.path())
        .args(["dash", "-i", "asset.json", "--filter", "type==text"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("no track matches the filter"),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn dash_requires_input() {
    let dir = tempfile::tempdir().unwrap();

    let status = dyndo(dir.path()).arg("dash").status().unwrap();

    assert!(!status.success());
}

#[test]
fn hls_requires_input() {
    let dir = tempfile::tempdir().unwrap();

    let status = dyndo(dir.path()).arg("hls").status().unwrap();

    assert!(!status.success());
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
