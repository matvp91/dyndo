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
    assert_eq!(tracks[0]["id"].as_str().unwrap().len(), 36);
    assert_eq!(tracks[1]["type"], "audio");
    assert_eq!(tracks[1]["language"], "nld");
    assert_eq!(tracks[0]["codec"], "avc1.640028");
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
fn generates_hls_playlists_in_output_directory() {
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

    let descriptor: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("asset.json")).unwrap()).unwrap();
    let track_ids = descriptor["tracks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|track| track["id"].as_str().unwrap())
        .collect::<Vec<_>>();

    assert!(
        dyndo(dir.path())
            .args(["hls", "-i", "asset.json", "-o", "playlists"])
            .status()
            .unwrap()
            .success()
    );

    let master = fs::read_to_string(dir.path().join("playlists/master.m3u8")).unwrap();
    assert!(
        track_ids.iter().all(|id| {
            master.contains(&format!("{id}.m3u8"))
                && dir.path().join(format!("playlists/{id}.m3u8")).is_file()
        }),
        "{master}"
    );
}

#[test]
fn dash_places_segment_template_on_adaptation_set() {
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
    assert!(xml.contains("$RepresentationID$/$Time$.m4s"));
    let first_rep = xml.find("<Representation").unwrap();
    let first_st = xml.find("<SegmentTemplate").unwrap();
    assert!(first_st < first_rep);
}

#[test]
fn indexes_raw_vtt_track() {
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
    assert_eq!(text["id"].as_str().unwrap().len(), 36);
    assert_eq!(text["codec"], "wvtt");
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
    let id = json["tracks"][0]["id"].as_str().unwrap().to_string();
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
    assert!(xml.contains(&format!("id=\"{id}\"")), "{xml}");
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
    assert_eq!(audio["id"].as_str().unwrap().len(), 36);
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
fn reindexing_an_existing_path_keeps_descriptor_metadata() {
    let dir = tempfile::tempdir().unwrap();
    stage(dir.path(), &["audio_aac_nl_2.mp4"]);

    assert!(
        dyndo(dir.path())
            .args(["index", "audio_aac_nl_2.mp4", "-o", "asset.json"])
            .status()
            .unwrap()
            .success()
    );

    // Hand-edit the descriptor language: the descriptor is authoritative.
    let path = dir.path().join("asset.json");
    let mut json: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    json["tracks"][0]["language"] = "fra".into();
    fs::write(&path, serde_json::to_vec_pretty(&json).unwrap()).unwrap();

    // Re-index the same path with no overrides: the edit must survive.
    assert!(
        dyndo(dir.path())
            .args(["index", "audio_aac_nl_2.mp4", "-o", "asset.json"])
            .status()
            .unwrap()
            .success()
    );

    let json: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let tracks = json["tracks"].as_array().unwrap();
    assert_eq!(tracks.len(), 1, "same path should not duplicate");
    assert_eq!(tracks[0]["language"], "fra", "{:?}", tracks[0]);
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
