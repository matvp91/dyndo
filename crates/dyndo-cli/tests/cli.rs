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
            "video_avc_1080.mp4",
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
    assert_eq!(tracks[0]["height"], 1080);
    assert_eq!(tracks[0]["path"], "video_avc_1080.mp4");
    // The derived representation id is pinned at index time.
    assert!(
        tracks[0]["id"].as_str().unwrap().starts_with("video_1080_"),
        "{:?}",
        tracks[0]["id"]
    );
    assert_eq!(tracks[1]["type"], "audio");
    assert_eq!(tracks[1]["language"], "nld");
    // Derived debug fields, recomputed from the probe on every write.
    assert_eq!(tracks[0]["mime_type"], "video/mp4");
    assert_eq!(tracks[0]["codec"], "avc1.640028");
    assert_eq!(tracks[1]["mime_type"], "audio/mp4");
    assert_eq!(tracks[1]["codec"], "mp4a.40.2");
}

#[test]
fn generates_mpd_from_asset_json() {
    let dir = tempfile::tempdir().unwrap();
    stage(dir.path(), &["video_avc_1080.mp4", "audio_aac_nl_2.mp4"]);

    assert!(
        dyndo(dir.path())
            .args([
                "index",
                "video_avc_1080.mp4",
                "audio_aac_nl_2.mp4",
                "-o",
                "asset.json",
            ])
            .status()
            .unwrap()
            .success()
    );

    assert!(
        dyndo(dir.path())
            .args(["dash", "-i", "asset.json", "-o", "stream.mpd"])
            .status()
            .unwrap()
            .success()
    );

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

    assert!(
        dyndo(dir.path())
            .args([
                "index",
                "video_avc_1080.mp4",
                "audio_aac_nl_2.mp4",
                "-o",
                "asset.json",
            ])
            .status()
            .unwrap()
            .success()
    );

    assert!(
        dyndo(dir.path())
            .args(["dash", "-i", "asset.json", "-o", "plain.mpd"])
            .status()
            .unwrap()
            .success()
    );

    assert!(
        dyndo(dir.path())
            .args(["dash", "-i", "asset.json", "-o", "compact.mpd", "-c"])
            .status()
            .unwrap()
            .success()
    );

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

    assert!(
        dyndo(dir.path())
            .args([
                "index",
                "video_avc_1080.mp4",
                "audio_aac_nl_2.mp4",
                "-o",
                "asset.json",
            ])
            .status()
            .unwrap()
            .success()
    );

    assert!(
        dyndo(dir.path())
            .args(["hls", "-i", "asset.json", "-o", "hls"])
            .status()
            .unwrap()
            .success()
    );

    // Master plus one media playlist per track (video + audio) = 3 files.
    let names: Vec<String> = fs::read_dir(dir.path().join("hls"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(names.iter().filter(|n| n.ends_with(".m3u8")).count(), 3);
    assert!(
        names
            .iter()
            .any(|n| n.starts_with("video_") && n.ends_with(".m3u8"))
    );
    assert!(
        names
            .iter()
            .any(|n| n.starts_with("audio_") && n.ends_with(".m3u8"))
    );

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
fn indexes_raw_vtt_track_without_advertising_it() {
    let dir = tempfile::tempdir().unwrap();
    stage(dir.path(), &["video_avc_1080.mp4", "text_sample.vtt"]);

    assert!(
        dyndo(dir.path())
            .args([
                "index",
                "video_avc_1080.mp4",
                "text_sample.vtt,language=eng",
                "-o",
                "asset.json",
            ])
            .status()
            .unwrap()
            .success()
    );

    // The raw VTT file indexes as a text track with the declared language.
    let json: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("asset.json")).unwrap()).unwrap();
    let tracks = json["tracks"].as_array().unwrap();
    assert_eq!(tracks.len(), 2);
    let text = tracks
        .iter()
        .find(|t| t["type"] == "text")
        .expect("a text track");
    assert_eq!(text["language"], "eng");
    assert_eq!(text["path"], "text_sample.vtt");
    // A raw file has a real MIME type but no RFC 6381 codec.
    assert_eq!(text["mime_type"], "text/vtt");
    assert!(text["codec"].is_null(), "{text:?}");

    // Raw (non-CMAF) tracks are not advertised in manifests yet.
    assert!(
        dyndo(dir.path())
            .args(["dash", "-i", "asset.json", "-o", "stream.mpd"])
            .status()
            .unwrap()
            .success()
    );
    let xml = fs::read_to_string(dir.path().join("stream.mpd")).unwrap();
    assert!(!xml.contains("contentType=\"text\""), "{xml}");

    assert!(
        dyndo(dir.path())
            .args(["hls", "-i", "asset.json", "-o", "hls"])
            .status()
            .unwrap()
            .success()
    );
    let master = fs::read_to_string(dir.path().join("hls/index.m3u8")).unwrap();
    assert!(!master.contains("TYPE=SUBTITLES"), "{master}");
}

#[test]
fn manual_language_edit_in_asset_json_overrides_probed_language() {
    let dir = tempfile::tempdir().unwrap();
    stage(dir.path(), &["audio_aac_nl_2.mp4"]);

    // Index the audio track: both the file's mdhd and asset.json say "nld".
    assert!(
        dyndo(dir.path())
            .args(["index", "audio_aac_nl_2.mp4", "-o", "asset.json"])
            .status()
            .unwrap()
            .success()
    );

    // Hand-edit the descriptor language to "fra": manifests must follow it
    // even though the file's mdhd still says "nld".
    let path = dir.path().join("asset.json");
    let mut json: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    json["tracks"][0]["language"] = "fra".into();
    fs::write(&path, serde_json::to_vec_pretty(&json).unwrap()).unwrap();

    assert!(
        dyndo(dir.path())
            .args(["dash", "-i", "asset.json", "-o", "stream.mpd"])
            .status()
            .unwrap()
            .success()
    );
    let xml = fs::read_to_string(dir.path().join("stream.mpd")).unwrap();
    assert!(xml.contains("lang=\"fra\""), "{xml}");
    assert!(!xml.contains("lang=\"nld\""), "{xml}");
    // The representation id stays the one stored in asset.json at index time —
    // segment routes look tracks up by that id, so a language edit must not
    // re-derive it.
    assert!(xml.contains("audio_nld_"), "{xml}");
    assert!(!xml.contains("audio_fra_"), "{xml}");

    assert!(
        dyndo(dir.path())
            .args(["hls", "-i", "asset.json", "-o", "hls"])
            .status()
            .unwrap()
            .success()
    );
    let master = fs::read_to_string(dir.path().join("hls/index.m3u8")).unwrap();
    assert!(master.contains("mp4a.40.2"), "{master}");
}

#[test]
fn index_sets_language_and_role_on_audio() {
    let dir = tempfile::tempdir().unwrap();
    stage(dir.path(), &["video_avc_1080.mp4", "audio_aac_nl_2.mp4"]);

    assert!(
        dyndo(dir.path())
            .args([
                "index",
                "video_avc_1080.mp4",
                "audio_aac_nl_2.mp4,language=fra,role=commentary",
                "-o",
                "asset.json",
            ])
            .status()
            .unwrap()
            .success()
    );

    let json: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("asset.json")).unwrap()).unwrap();
    let audio = json["tracks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["type"] == "audio")
        .expect("an audio track");
    assert_eq!(audio["language"], "fra"); // probed nld, overridden
    assert_eq!(audio["role"], "commentary");
}

#[test]
fn index_appends_a_new_track_to_an_existing_descriptor() {
    let dir = tempfile::tempdir().unwrap();
    stage(dir.path(), &["video_avc_1080.mp4", "audio_aac_nl_2.mp4"]);

    assert!(
        dyndo(dir.path())
            .args(["index", "video_avc_1080.mp4", "-o", "asset.json"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        dyndo(dir.path())
            .args(["index", "audio_aac_nl_2.mp4", "-o", "asset.json"])
            .status()
            .unwrap()
            .success()
    );

    let json: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("asset.json")).unwrap()).unwrap();
    let tracks = json["tracks"].as_array().unwrap();
    assert_eq!(tracks.len(), 2, "second index should append, not overwrite");
    assert!(tracks.iter().any(|t| t["type"] == "video"));
    assert!(tracks.iter().any(|t| t["type"] == "audio"));
}

#[test]
fn index_upserts_an_existing_path_in_place() {
    let dir = tempfile::tempdir().unwrap();
    stage(dir.path(), &["audio_aac_nl_2.mp4"]);

    // First index: no role.
    assert!(
        dyndo(dir.path())
            .args(["index", "audio_aac_nl_2.mp4", "-o", "asset.json"])
            .status()
            .unwrap()
            .success()
    );
    // Re-index the same path, now declaring a role.
    assert!(
        dyndo(dir.path())
            .args(["index", "audio_aac_nl_2.mp4,role=main", "-o", "asset.json"])
            .status()
            .unwrap()
            .success()
    );

    let json: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("asset.json")).unwrap()).unwrap();
    let tracks = json["tracks"].as_array().unwrap();
    assert_eq!(tracks.len(), 1, "same path should replace, not duplicate");
    assert_eq!(tracks[0]["role"], "main");
}

#[test]
fn index_rejects_role_on_video() {
    let dir = tempfile::tempdir().unwrap();
    stage(dir.path(), &["video_avc_1080.mp4"]);
    assert!(
        !dyndo(dir.path())
            .args(["index", "video_avc_1080.mp4,role=main", "-o", "asset.json"])
            .status()
            .unwrap()
            .success()
    );
}

#[test]
fn index_rejects_a_text_role_on_audio() {
    let dir = tempfile::tempdir().unwrap();
    stage(dir.path(), &["audio_aac_nl_2.mp4"]);
    assert!(
        !dyndo(dir.path())
            .args([
                "index",
                "audio_aac_nl_2.mp4,role=subtitle",
                "-o",
                "asset.json"
            ])
            .status()
            .unwrap()
            .success()
    );
}

#[test]
fn index_rejects_an_unknown_field() {
    let dir = tempfile::tempdir().unwrap();
    stage(dir.path(), &["audio_aac_nl_2.mp4"]);
    assert!(
        !dyndo(dir.path())
            .args(["index", "audio_aac_nl_2.mp4,codec=aac", "-o", "asset.json"])
            .status()
            .unwrap()
            .success()
    );
}

#[test]
fn index_rejects_path_used_as_a_key() {
    let dir = tempfile::tempdir().unwrap();
    stage(dir.path(), &["video_avc_1080.mp4"]);
    assert!(
        !dyndo(dir.path())
            .args(["index", "path=video_avc_1080.mp4", "-o", "asset.json"])
            .status()
            .unwrap()
            .success()
    );
}
