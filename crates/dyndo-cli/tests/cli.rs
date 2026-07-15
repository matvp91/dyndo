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

#[test]
fn generates_hls_playlists_from_asset_json() {
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
        .args(["hls", "-i", "asset.json", "-o", "hls"])
        .status()
        .unwrap()
        .success());

    // Master plus one media playlist per track (video + audio) = 3 files.
    let names: Vec<String> = fs::read_dir(dir.path().join("hls"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(names.iter().filter(|n| n.ends_with(".m3u8")).count(), 3);
    assert!(names
        .iter()
        .any(|n| n.starts_with("video_") && n.ends_with(".m3u8")));
    assert!(names
        .iter()
        .any(|n| n.starts_with("audio_") && n.ends_with(".m3u8")));

    let master = fs::read_to_string(dir.path().join("hls/index.m3u8")).unwrap();
    assert!(master.contains("#EXT-X-STREAM-INF:"));
    assert!(master.contains("#EXT-X-MEDIA:TYPE=AUDIO"));
    assert!(master.contains("AUDIO=\"mp4a\""));

    let video = names
        .iter()
        .find(|n| n.starts_with("video_") && n.ends_with(".m3u8"))
        .unwrap();
    let media = fs::read_to_string(dir.path().join("hls").join(video)).unwrap();
    assert!(media.contains("#EXT-X-PLAYLIST-TYPE:VOD"));
    assert!(media.contains("#EXT-X-MAP:URI="));
    assert!(media.contains("#EXT-X-ENDLIST"));
}

#[test]
fn advertises_text_track_in_dash_and_hls() {
    let dir = tempfile::tempdir().unwrap();
    stage(
        dir.path(),
        &[
            "video_avc_1080.mp4",
            "audio_aac_nl_2.mp4",
            "text_sample.vtt",
        ],
    );

    // Index video + audio, then pack the subtitle against the asset (which adds
    // the text track to asset.json itself).
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
        .args([
            "pack",
            "-i",
            "text_sample.vtt",
            "-a",
            "asset.json",
            "-l",
            "eng"
        ])
        .status()
        .unwrap()
        .success());

    // HLS: the master advertises the subtitle group and a text media playlist.
    assert!(dyndo(dir.path())
        .args(["hls", "-i", "asset.json", "-o", "hls"])
        .status()
        .unwrap()
        .success());

    let master = fs::read_to_string(dir.path().join("hls/index.m3u8")).unwrap();
    assert!(master.contains("#EXT-X-MEDIA:TYPE=SUBTITLES"), "{master}");
    assert!(master.contains("SUBTITLES=\"wvtt\""), "{master}");

    let names: Vec<String> = fs::read_dir(dir.path().join("hls"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    let text_playlist = names
        .iter()
        .find(|n| n.starts_with("text_wvtt_") && n.ends_with(".m3u8"))
        .expect("a text media playlist");
    let media = fs::read_to_string(dir.path().join("hls").join(text_playlist)).unwrap();
    assert!(media.contains("#EXT-X-PLAYLIST-TYPE:VOD"), "{media}");
    assert!(media.contains("#EXT-X-MAP:URI="), "{media}");

    // DASH: the text AdaptationSet carries the subtitle role.
    assert!(dyndo(dir.path())
        .args(["dash", "-i", "asset.json", "-o", "stream.mpd"])
        .status()
        .unwrap()
        .success());
    let xml = fs::read_to_string(dir.path().join("stream.mpd")).unwrap();
    assert!(xml.contains("contentType=\"text\""), "{xml}");
    assert!(xml.contains("value=\"subtitle\""), "{xml}");
}

#[test]
fn indexes_wvtt_text_track() {
    let dir = tempfile::tempdir().unwrap();
    stage(dir.path(), &["video_avc_1080.mp4", "text_sample.vtt"]);

    assert!(dyndo(dir.path())
        .args(["index", "-i", "video_avc_1080.mp4", "-o", "asset.json"])
        .status()
        .unwrap()
        .success());
    assert!(dyndo(dir.path())
        .args([
            "pack",
            "-i",
            "text_sample.vtt",
            "-a",
            "asset.json",
            "-l",
            "eng"
        ])
        .status()
        .unwrap()
        .success());

    // Index the packed wvtt file on its own into a fresh descriptor.
    assert!(dyndo(dir.path())
        .args(["index", "-i", "text_wvtt_eng.mp4", "-o", "text.json"])
        .status()
        .unwrap()
        .success());

    let json: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("text.json")).unwrap()).unwrap();
    let tracks = json["tracks"].as_array().unwrap();
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0]["type"], "text");
    assert_eq!(tracks[0]["fourcc"], "wvtt");
    assert_eq!(tracks[0]["path"], "text_wvtt_eng.mp4");
}

#[test]
fn pack_aligns_subtitles_to_video_and_updates_asset() {
    let dir = tempfile::tempdir().unwrap();
    stage(dir.path(), &["video_avc_1080.mp4", "sample.vtt"]);

    // Index the video so pack has a timeline to align to.
    assert!(dyndo(dir.path())
        .args(["index", "-i", "video_avc_1080.mp4", "-o", "asset.json"])
        .status()
        .unwrap()
        .success());

    // Pack the subtitle against the asset's first video track.
    assert!(dyndo(dir.path())
        .args(["pack", "-i", "sample.vtt", "-a", "asset.json", "-l", "eng"])
        .status()
        .unwrap()
        .success());

    // <id>.mp4 is written beside the descriptor and is a valid wvtt MP4.
    let data = fs::read(dir.path().join("text_wvtt_eng.mp4")).unwrap();
    assert!(data.len() > 8, "expected a non-trivial mp4");
    assert_eq!(&data[4..8], b"ftyp");
    assert!(data.windows(4).any(|w| w == b"wvtt"));

    // asset.json now lists the text track.
    let json: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("asset.json")).unwrap()).unwrap();
    let tracks = json["tracks"].as_array().unwrap();
    let text = tracks
        .iter()
        .find(|t| t["type"] == "text")
        .expect("a text track in the updated asset");
    assert_eq!(text["fourcc"], "wvtt");
    assert_eq!(text["language"], "eng");
    assert_eq!(text["path"], "text_wvtt_eng.mp4");
}

#[test]
fn pack_without_a_video_track_fails() {
    let dir = tempfile::tempdir().unwrap();
    stage(dir.path(), &["audio_aac_nl_2.mp4", "sample.vtt"]);

    // An audio-only asset has no video timeline to align to.
    assert!(dyndo(dir.path())
        .args(["index", "-i", "audio_aac_nl_2.mp4", "-o", "asset.json"])
        .status()
        .unwrap()
        .success());

    let status = dyndo(dir.path())
        .args(["pack", "-i", "sample.vtt", "-a", "asset.json", "-l", "eng"])
        .status()
        .unwrap();
    assert!(!status.success(), "pack should fail without a video track");
}
