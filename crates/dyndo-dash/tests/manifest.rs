use std::path::PathBuf;
use std::time::Duration;

use dyndo_core::asset_descriptor::AssetDescriptor;
use dyndo_dash::options::DashOptions;
use opendal::Operator;
use opendal::services::Memory;
use serde::Serialize;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures");

#[tokio::test]
async fn generate_mpd_emits_complete_vod_manifest() {
    let (op, asset) = asset().await;

    let mpd = dyndo_dash::builder::generate_mpd(&op, &asset, &DashOptions::default(), None)
        .await
        .unwrap();
    let mut xml = String::new();
    let mut serializer = quick_xml::se::Serializer::new(&mut xml);
    serializer.indent(' ', 2);
    mpd.serialize(serializer).unwrap();

    for expected in [
        "type=\"static\"",
        "mediaPresentationDuration=\"PT",
        "minBufferTime=\"PT",
        "contentType=\"video\"",
        "contentType=\"audio\"",
        "segmentAlignment=\"true\"",
        "startWithSAP=\"1\"",
        "id=\"video-main\"",
        "codecs=\"avc1.640028\"",
        "width=\"1920\"",
        "height=\"1080\"",
        "frameRate=\"25/1\"",
        "id=\"audio-nld\"",
        "codecs=\"mp4a.40.2\"",
        "audioSamplingRate=\"48000\"",
        "lang=\"nld\"",
        "initialization=\"$RepresentationID$/init.mp4\"",
        "media=\"$RepresentationID$/$Time$.m4s\"",
        "<SegmentTimeline>",
    ] {
        assert!(xml.contains(expected), "missing {expected:?} in {xml}");
    }
}

/// The descriptor's declared codec is not trusted: a track re-encoded without being
/// re-indexed would otherwise be advertised as what it used to be.
#[tokio::test]
async fn generate_mpd_advertises_the_probed_codec_over_the_declared_one() {
    let (op, mut asset) = asset().await;
    asset.tracks[0].codec = "avc1.stale".to_string();

    let mpd = dyndo_dash::builder::generate_mpd(&op, &asset, &DashOptions::default(), None)
        .await
        .unwrap();
    let mut xml = String::new();
    let mut serializer = quick_xml::se::Serializer::new(&mut xml);
    serializer.indent(' ', 2);
    mpd.serialize(serializer).unwrap();

    assert!(
        xml.contains("codecs=\"avc1.640028\"") && !xml.contains("avc1.stale"),
        "unexpected manifest: {xml}"
    );
}

#[tokio::test]
async fn generate_mpd_applies_the_assets_minimum_segment_length() {
    let (op, mut asset) = asset().await;
    asset.segment_options.min_length = 10_000;

    let mpd = dyndo_dash::builder::generate_mpd(&op, &asset, &DashOptions::default(), None)
        .await
        .unwrap();
    let mut xml = String::new();
    mpd.serialize(quick_xml::se::Serializer::new(&mut xml))
        .unwrap();

    assert!(xml.contains("d=\"1036800\""));
}

#[tokio::test]
async fn generate_mpd_keeps_templates_on_representations_when_not_compact() {
    let (op, asset) = asset().await;

    let mpd = dyndo_dash::builder::generate_mpd(&op, &asset, &DashOptions::default(), None)
        .await
        .unwrap();

    assert!(mpd.periods[0].adaptations.iter().all(|adaptation_set| {
        adaptation_set.SegmentTemplate.is_none()
            && adaptation_set
                .representations
                .iter()
                .all(|representation| representation.SegmentTemplate.is_some())
    }));
}

#[tokio::test]
async fn generate_mpd_hoists_templates_when_compact() {
    let (op, asset) = asset().await;

    let mpd = dyndo_dash::builder::generate_mpd(
        &op,
        &asset,
        &DashOptions {
            compact: true,
            ..DashOptions::default()
        },
        None,
    )
    .await
    .unwrap();

    assert!(mpd.periods[0].adaptations.iter().all(|adaptation_set| {
        adaptation_set.SegmentTemplate.is_some()
            && adaptation_set
                .representations
                .iter()
                .all(|representation| representation.SegmentTemplate.is_none())
    }));
}

#[tokio::test]
async fn generate_mpd_leaves_one_period_until_multi_period_is_asked_for() {
    let (op, asset) = spliced_asset().await;

    let mpd = dyndo_dash::builder::generate_mpd(&op, &asset, &DashOptions::default(), None)
        .await
        .unwrap();

    assert_eq!(mpd.periods.len(), 1);
}

#[tokio::test]
async fn generate_mpd_opens_a_period_at_each_boundary() {
    let (op, asset) = spliced_asset().await;

    let mpd = dyndo_dash::builder::generate_mpd(&op, &asset, &multi_period(), None)
        .await
        .unwrap();

    assert_eq!(
        mpd.periods
            .iter()
            .map(|period| (period.id.as_deref(), period.start, period.duration))
            .collect::<Vec<_>>(),
        vec![
            (
                Some("0"),
                Some(Duration::ZERO),
                Some(Duration::from_millis(3_840))
            ),
            (
                Some("1"),
                Some(Duration::from_millis(3_840)),
                Some(Duration::from_millis(7_680))
            ),
        ]
    );
}

/// A period is offset to the boundary and never to the cut a track snapped to, so
/// a track cutting late keeps presenting where it always did rather than being
/// pulled back to the period edge. Anchoring on the track instead would re-time
/// it against its siblings for the rest of the presentation.
#[tokio::test]
async fn generate_mpd_offsets_a_period_to_the_boundary_not_to_a_tracks_own_cut() {
    let (op, asset) = spliced_asset().await;

    let mpd = dyndo_dash::builder::generate_mpd(&op, &asset, &multi_period(), None)
        .await
        .unwrap();
    let offsets: Vec<(u64, u64)> = mpd.periods[1]
        .adaptations
        .iter()
        .map(|adaptation_set| {
            let template = adaptation_set.representations[0]
                .SegmentTemplate
                .as_ref()
                .unwrap();
            (
                template.presentationTimeOffset.unwrap(),
                template.SegmentTimeline.as_ref().unwrap().segments[0]
                    .t
                    .unwrap(),
            )
        })
        .collect();

    // Video's grid holds the boundary exactly. Audio's, at 44.1 kHz, cannot reach
    // it and cuts 640 samples later, which it states by starting its timeline
    // past the offset rather than by moving the offset.
    assert_eq!(offsets, vec![(49_152, 49_152), (169_344, 169_984)]);
}

#[tokio::test]
async fn generate_mpd_declares_a_period_continuous_with_the_one_before_it() {
    let (op, asset) = spliced_asset().await;

    let mpd = dyndo_dash::builder::generate_mpd(&op, &asset, &multi_period(), None)
        .await
        .unwrap();

    assert!(
        mpd.periods[0]
            .adaptations
            .iter()
            .all(|adaptation_set| adaptation_set.supplemental_property.is_empty())
    );
    assert!(mpd.periods[1].adaptations.iter().all(|adaptation_set| {
        adaptation_set.supplemental_property.iter().any(|property| {
            property.schemeIdUri == "urn:mpeg:dash:period-continuity:2015"
                && property.value.as_deref() == Some("0")
        })
    }));
}

/// Two boundaries close enough to snap to the same segment edge on every track
/// leave the span between them with nothing to serve, so no period opens there.
#[tokio::test]
async fn generate_mpd_skips_a_period_no_track_can_cut_for() {
    let (op, mut asset) = spliced_asset().await;
    asset.segment_options.boundaries = vec![3_800, 3_840];

    let mpd = dyndo_dash::builder::generate_mpd(&op, &asset, &multi_period(), None)
        .await
        .unwrap();

    assert_eq!(
        mpd.periods
            .iter()
            .map(|period| (period.id.as_deref(), period.start))
            .collect::<Vec<_>>(),
        vec![
            (Some("0"), Some(Duration::ZERO)),
            (Some("1"), Some(Duration::from_millis(3_840))),
        ]
    );
}

/// A track ending before a span opens drops out of that period alone. The ids of
/// the AdaptationSets around it stay put, since they are what a client matches a
/// rendition on from one period to the next.
#[tokio::test]
async fn generate_mpd_drops_a_track_that_ended_and_keeps_the_other_ids() {
    let (op, asset) = subtitled_spliced_asset().await;

    let mpd = dyndo_dash::builder::generate_mpd(&op, &asset, &multi_period(), None)
        .await
        .unwrap();

    assert_eq!(
        adaptation_set_ids(&mpd.periods[0]),
        vec!["0", "1", "2"],
        "the subtitles are the first AdaptationSet while they last"
    );
    assert_eq!(adaptation_set_ids(&mpd.periods[1]), vec!["1", "2"]);
}

/// The tracks that survive keep continuing the AdaptationSet holding their id,
/// which is only true because dropping the subtitles left those ids alone.
#[tokio::test]
async fn generate_mpd_keeps_continuity_across_a_dropped_track() {
    let (op, asset) = subtitled_spliced_asset().await;

    let mpd = dyndo_dash::builder::generate_mpd(&op, &asset, &multi_period(), None)
        .await
        .unwrap();

    assert!(mpd.periods[1].adaptations.iter().all(|adaptation_set| {
        adaptation_set.supplemental_property.iter().any(|property| {
            property.schemeIdUri == "urn:mpeg:dash:period-continuity:2015"
                && property.value.as_deref() == Some("0")
        })
    }));
}

fn adaptation_set_ids(period: &dash_mpd::Period) -> Vec<&str> {
    period
        .adaptations
        .iter()
        .map(|adaptation_set| adaptation_set.id.as_deref().unwrap())
        .collect()
}

#[tokio::test]
async fn generate_mpd_hands_every_segment_to_exactly_one_period() {
    let (op, asset) = spliced_asset().await;

    let single = dyndo_dash::builder::generate_mpd(&op, &asset, &DashOptions::default(), None)
        .await
        .unwrap();
    let split = dyndo_dash::builder::generate_mpd(&op, &asset, &multi_period(), None)
        .await
        .unwrap();

    assert_eq!(segment_counts(&split), segment_counts(&single));
}

/// The segments each AdaptationSet holds, summed over every period.
fn segment_counts(mpd: &dash_mpd::MPD) -> Vec<u64> {
    let mut counts = vec![0; mpd.periods[0].adaptations.len()];
    for period in &mpd.periods {
        for (index, adaptation_set) in period.adaptations.iter().enumerate() {
            let template = adaptation_set.representations[0]
                .SegmentTemplate
                .as_ref()
                .unwrap();
            counts[index] += template
                .SegmentTimeline
                .as_ref()
                .unwrap()
                .segments
                .iter()
                .map(|segment| segment.r.unwrap_or(0) as u64 + 1)
                .sum::<u64>();
        }
    }
    counts
}

fn multi_period() -> DashOptions {
    DashOptions {
        multi_period: true,
        ..DashOptions::default()
    }
}

/// Video on a 1.92 s grid against audio at 44.1 kHz, which cannot land on it, so
/// the two cut at different times for the same boundary.
async fn spliced_asset() -> (Operator, AssetDescriptor) {
    let op = Operator::new(Memory::default()).unwrap();
    stage(&op, "video_avc_144.mp4").await;
    stage(&op, "audio_aac_nl_2_44100.mp4").await;
    op.write(
        "asset.json",
        r#"{
          "segment_options": { "boundaries": [3840] },
          "tracks": [
            {
              "id": "video-main",
              "path": "video_avc_144.mp4",
              "codec": "avc1.64000c",
              "type": "video",
              "width": 256,
              "height": 144,
              "frame_rate": "25/1"
            },
            {
              "id": "audio-nld",
              "path": "audio_aac_nl_2_44100.mp4",
              "codec": "mp4a.40.2",
              "type": "audio",
              "sample_rate": 44100,
              "channels": 2,
              "language": "nld"
            }
          ]
        }"#,
    )
    .await
    .unwrap();
    let asset = AssetDescriptor::read(&op, "asset.json").await.unwrap();
    (op, asset)
}

/// The same asset with subtitles ending long before it does, listed first so that
/// dropping them would renumber the tracks that outlast them.
async fn subtitled_spliced_asset() -> (Operator, AssetDescriptor) {
    let (op, mut asset) = spliced_asset().await;
    op.write(
        "subtitles_nld.vtt",
        "WEBVTT\n\n00:00.000 --> 00:02.000\nHello\n",
    )
    .await
    .unwrap();
    asset.tracks.insert(
        0,
        serde_json::from_value(serde_json::json!({
            "id": "text-nld",
            "path": "subtitles_nld.vtt",
            "codec": "wvtt",
            "type": "text",
            "language": "nld"
        }))
        .unwrap(),
    );

    (op, asset)
}

async fn asset() -> (Operator, AssetDescriptor) {
    let op = Operator::new(Memory::default()).unwrap();
    stage(&op, "video_avc_1080.mp4").await;
    stage(&op, "audio_aac_nl_2.mp4").await;
    op.write(
        "asset.json",
        r#"{
          "tracks": [
            {
              "id": "video-main",
              "path": "video_avc_1080.mp4",
              "codec": "avc1.640028",
              "type": "video",
              "width": 1920,
              "height": 1080,
              "frame_rate": "25/1"
            },
            {
              "id": "audio-nld",
              "path": "audio_aac_nl_2.mp4",
              "codec": "mp4a.40.2",
              "type": "audio",
              "sample_rate": 48000,
              "channels": 2,
              "language": "nld"
            }
          ]
        }"#,
    )
    .await
    .unwrap();
    let asset = AssetDescriptor::read(&op, "asset.json").await.unwrap();
    (op, asset)
}

async fn stage(op: &Operator, name: &str) {
    let bytes = std::fs::read(PathBuf::from(FIXTURES).join(name)).unwrap();
    op.write(name, bytes).await.unwrap();
}
