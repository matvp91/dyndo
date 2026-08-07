mod context;
mod filter;
mod segment;
mod transport;

use axum::{
    Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::Response,
    routing::get,
};
use dyndo_dash::options::DashOptions;
use dyndo_hls::options::HlsOptions;
use opendal::Operator;
use tower_http::cors::{Any, CorsLayer};

use crate::error::ServerError;
use crate::routes::context::parse_context;
use crate::routes::filter::FilterQuery;

pub(crate) fn build_router(op: Operator) -> Router {
    let cors = CorsLayer::new().allow_origin(Any).allow_methods(Any);
    Router::new()
        .route("/health", get(health))
        // The two shapes an output path takes: something naming the whole asset, and
        // one file of one track. Each resource is then named by its extension rather
        // than assumed from what is left, since a route parameter spans a whole path
        // segment and so cannot carry the extension itself.
        .route("/out/{options}/{resource}", get(manifest))
        .route("/out/{options}/{track_id}/{file}", get(track_file))
        .with_state(op)
        .layer(cors)
}

async fn health() -> StatusCode {
    StatusCode::OK
}

/// A manifest describing the whole asset, or the media playlist of one track.
///
/// Only the whole-asset manifests read the filter: a media playlist describes one
/// track, so narrowing the set cannot change it.
async fn manifest(
    State(op): State<Operator>,
    Path((options, resource)): Path<(String, String)>,
    Query(query): Query<FilterQuery>,
) -> Result<Response, ServerError> {
    let not_found = || ServerError::NotFound(resource.clone());

    match resource.rsplit_once('.').ok_or_else(not_found)? {
        ("index", "mpd") => {
            let context = parse_context::<DashOptions>(&options)?;
            let filter = query.resolve()?;
            transport::dash_manifest(&op, &context, filter.as_ref()).await
        }
        ("master", "m3u8") => {
            let context = parse_context::<HlsOptions>(&options)?;
            let filter = query.resolve()?;
            transport::hls_master(&op, &context, filter.as_ref()).await
        }
        (track_id, "m3u8") => {
            let context = parse_context::<HlsOptions>(&options)?;
            transport::hls_media(&op, &context, track_id).await
        }
        _ => Err(not_found()),
    }
}

/// One file of one track: its initialization segment, or one media segment as either
/// the packaged bytes it is stored as or the WebVTT document those bytes hold.
async fn track_file(
    State(op): State<Operator>,
    Path((options, track_id, file)): Path<(String, String, String)>,
) -> Result<Response, ServerError> {
    let context = parse_context::<()>(&options)?;
    let not_found = || ServerError::NotFound(file.clone());

    match file.rsplit_once('.').ok_or_else(not_found)? {
        ("init", "mp4") => segment::initialization(&op, &context, &track_id).await,
        (time, "m4s") => segment::media(&op, &context, &track_id, segment_time(time, &file)?).await,
        (time, "vtt") => segment::text(&op, &context, &track_id, segment_time(time, &file)?).await,
        _ => Err(not_found()),
    }
}

/// The presentation time a segment filename names.
fn segment_time(name: &str, file: &str) -> Result<u64, ServerError> {
    name.parse()
        .map_err(|_| ServerError::NotFound(file.to_string()))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use opendal::services::Fs;
    use tempfile::TempDir;
    use tower::ServiceExt;

    use super::*;

    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures");

    #[tokio::test]
    async fn health_route_returns_ok() {
        let (_dir, app) = app("asset");

        let response = request(app, "/health").await;

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn dash_route_generates_manifest() {
        let (_dir, app) = app("asset");

        let response = request(app, "/out/(asset:asset)/index.mpd").await;
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();

        assert!(status == StatusCode::OK && String::from_utf8_lossy(&body).contains("<MPD"));
    }

    #[tokio::test]
    async fn hls_master_route_generates_playlist() {
        let (_dir, app) = app("asset");

        let response = request(app, "/out/(asset:asset)/master.m3u8").await;
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();

        assert!(status == StatusCode::OK && body.starts_with(b"#EXTM3U"));
    }

    #[tokio::test]
    async fn hls_media_route_generates_track_playlist() {
        let (_dir, app) = app("asset");

        let response = request(app, "/out/(asset:asset)/video-main.m3u8").await;
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();

        assert!(
            status == StatusCode::OK && String::from_utf8_lossy(&body).contains("#EXT-X-ENDLIST")
        );
    }

    #[tokio::test]
    async fn hls_media_route_applies_sml_alias() {
        let (_dir, app) = app("asset");

        let response = request(app, "/out/(asset:asset,sml:10000)/video-main.m3u8").await;
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();

        assert!(
            status == StatusCode::OK
                && String::from_utf8_lossy(&body).contains("#EXT-X-TARGETDURATION:12")
        );
    }

    #[tokio::test]
    async fn initialization_route_returns_track_bytes() {
        let (_dir, app) = app("asset");

        let response = request(app, "/out/(asset:asset)/video-main/init.mp4").await;
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();

        assert!(status == StatusCode::OK && !body.is_empty());
    }

    #[tokio::test]
    async fn text_segment_route_returns_a_webvtt_document() {
        let (_dir, app) = app("asset");

        let response = request(app, "/out/(asset:asset)/text-nld/0.vtt").await;
        let status = response.status();
        let content_type = response
            .headers()
            .get("content-type")
            .map(|value| value.to_str().unwrap().to_string());
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();

        assert_eq!(
            (status, content_type.as_deref(), body.starts_with(b"WEBVTT")),
            (StatusCode::OK, Some("text/vtt"), true),
            "unexpected body: {}",
            String::from_utf8_lossy(&body)
        );
    }

    #[tokio::test]
    async fn text_segment_route_serves_the_cues_the_document_holds() {
        let (_dir, app) = app("asset");

        let response = request(app, "/out/(asset:asset)/text-nld/0.vtt").await;
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let document = String::from_utf8(body.to_vec()).unwrap();

        assert!(
            document.contains("00:00:00.000 --> 00:00:02.000\nHello"),
            "unexpected body: {document}"
        );
    }

    /// The same segment stays addressable as packaged bytes, which is what DASH
    /// asks for while HLS asks for the document.
    #[tokio::test]
    async fn media_segment_route_still_serves_a_text_track_as_packaged_bytes() {
        let (_dir, app) = app("asset");

        let response = request(app, "/out/(asset:asset)/text-nld/0.m4s").await;
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();

        assert_eq!((status, &body[4..8]), (StatusCode::OK, b"styp".as_slice()));
    }

    #[tokio::test]
    async fn hls_media_route_points_a_text_track_at_vtt_segments() {
        let (_dir, app) = app("asset");

        let response = request(app, "/out/(asset:asset)/text-nld.m3u8").await;
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let playlist = String::from_utf8(body.to_vec()).unwrap();

        assert!(
            playlist.contains("text-nld/0.vtt") && !playlist.contains("EXT-X-MAP"),
            "unexpected playlist: {playlist}"
        );
    }

    #[tokio::test]
    async fn hls_media_route_points_a_text_track_at_packaged_segments_on_request() {
        let (_dir, app) = app("asset");

        let response = request(app, "/out/(asset:asset,wvtt:!t)/text-nld.m3u8").await;
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let playlist = String::from_utf8(body.to_vec()).unwrap();

        assert!(
            playlist.contains("text-nld/0.m4s") && playlist.contains("text-nld/init.mp4"),
            "unexpected playlist: {playlist}"
        );
    }

    #[tokio::test]
    async fn a_nested_asset_path_resolves() {
        let (_dir, app) = app("foo/asset");

        let response = request(app, "/out/(asset:foo%2Fasset)/index.mpd").await;

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn dash_route_omits_a_filtered_track() {
        let (_dir, app) = app("asset");

        let mpd = body(request(app, "/out/(asset:asset)/index.mpd?f=type!=video").await).await;

        assert!(
            !mpd.contains("video-main") && mpd.contains("audio-nld") && mpd.contains("text-nld"),
            "unexpected manifest: {mpd}"
        );
    }

    #[tokio::test]
    async fn hls_master_route_omits_a_filtered_track() {
        let (_dir, app) = app("asset");

        let playlist =
            body(request(app, "/out/(asset:asset)/master.m3u8?f=type==video").await).await;

        assert!(
            playlist.contains("video-main")
                && !playlist.contains("audio-nld")
                && !playlist.contains("text-nld"),
            "unexpected playlist: {playlist}"
        );
    }

    /// Filtering reads probed tracks, so the codec compared against is the one the
    /// source actually declares rather than the descriptor's claim.
    #[tokio::test]
    async fn a_filter_matches_a_probed_attribute() {
        let (_dir, app) = app("asset");

        let mpd =
            body(request(app, "/out/(asset:asset)/index.mpd?f=codec==avc1.640028").await).await;

        assert!(
            mpd.contains("video-main") && !mpd.contains("audio-nld"),
            "unexpected manifest: {mpd}"
        );
    }

    /// A comparison against an attribute a track does not carry is false, so a
    /// resolution cap on its own takes the audio and text tracks with it.
    #[tokio::test]
    async fn a_filter_drops_tracks_lacking_the_attribute() {
        let (_dir, app) = app("asset");

        let mpd = body(request(app, "/out/(asset:asset)/index.mpd?f=height%3C=1080").await).await;

        assert!(
            mpd.contains("video-main") && !mpd.contains("audio-nld") && !mpd.contains("text-nld"),
            "unexpected manifest: {mpd}"
        );
    }

    /// The `type!=…` idiom is how a filter spares the types it does not mean to
    /// judge, which is what keeps a resolution cap from stripping the audio.
    #[tokio::test]
    async fn a_spared_type_survives_a_cap_it_cannot_satisfy() {
        let (_dir, app) = app("asset");

        let playlist = body(
            request(
                app,
                "/out/(asset:asset)/master.m3u8?f=type!=video||height%3C=720",
            )
            .await,
        )
        .await;

        assert!(
            !playlist.contains("video-main")
                && playlist.contains("audio-nld")
                && playlist.contains("text-nld"),
            "unexpected playlist: {playlist}"
        );
    }

    #[tokio::test]
    async fn both_filter_spellings_agree() {
        let (_dir, app) = app("asset");

        let short =
            body(request(app.clone(), "/out/(asset:asset)/index.mpd?f=type==audio").await).await;
        let long =
            body(request(app, "/out/(asset:asset)/index.mpd?filter=type==audio").await).await;

        assert_eq!(short, long);
    }

    #[tokio::test]
    async fn a_filter_matching_no_track_returns_not_found() {
        let (_dir, app) = app("asset");

        let response = request(app, "/out/(asset:asset)/index.mpd?f=height%3C=720").await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn a_malformed_filter_returns_bad_request() {
        let (_dir, app) = app("asset");

        for filter in [
            "heigth%3C=720",
            "language%3Cnl",
            "height==tall",
            "height%3C=720(",
        ] {
            let uri = format!("/out/(asset:asset)/index.mpd?f={filter}");
            let response = request(app.clone(), &uri).await;

            assert_eq!(response.status(), StatusCode::BAD_REQUEST, "for {filter}");
        }
    }

    #[tokio::test]
    async fn passing_both_filter_spellings_returns_bad_request() {
        let (_dir, app) = app("asset");

        let response = request(
            app,
            "/out/(asset:asset)/index.mpd?f=type==video&filter=type==audio",
        )
        .await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    /// An unencoded `&&` splits the query string, leaving `f=type!=video` — which
    /// parses on its own, so it would otherwise serve a filter nobody asked for. The
    /// junk halves arrive as unknown parameters, which is what refuses the request.
    #[tokio::test]
    async fn an_unencoded_conjunction_returns_bad_request() {
        let (_dir, app) = app("asset");

        let response = request(
            app,
            "/out/(asset:asset)/index.mpd?f=type!=video&&height%3C=720",
        )
        .await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    /// A manifest request takes no parameter but the filter, which is what makes the
    /// unencoded-`&&` case above detectable rather than silently truncating.
    #[tokio::test]
    async fn an_unknown_query_parameter_returns_bad_request() {
        let (_dir, app) = app("asset");

        let response = request(app, "/out/(asset:asset)/index.mpd?sml=6000").await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    /// A manifest's relative URIs carry no query string, so the routes they address
    /// never see a filter and must not begin depending on one.
    #[tokio::test]
    async fn a_query_string_is_ignored_off_the_manifest_routes() {
        let (_dir, app) = app("asset");

        for uri in [
            "/out/(asset:asset)/video-main.m3u8?f=type==audio",
            "/out/(asset:asset)/video-main/init.mp4?f=type==audio",
            "/out/(asset:asset)/text-nld/0.vtt?f=heigth%3C=720",
        ] {
            let response = request(app.clone(), uri).await;

            assert_eq!(response.status(), StatusCode::OK, "for {uri}");
        }
    }

    /// The extension names the resource, so anything else is addressed at nothing
    /// rather than falling through to a handler.
    #[tokio::test]
    async fn an_unnamed_resource_returns_not_found() {
        let (_dir, app) = app("asset");

        for uri in [
            "/out/(asset:asset)/index.txt",
            "/out/(asset:asset)/index",
            "/out/(asset:asset)/video-main/0.txt",
            "/out/(asset:asset)/video-main/0",
            "/out/(asset:asset)/video-main/0.m4s/extra",
        ] {
            let response = request(app.clone(), uri).await;

            assert_eq!(response.status(), StatusCode::NOT_FOUND, "for {uri}");
        }
    }

    #[tokio::test]
    async fn malformed_options_return_bad_request() {
        let (_dir, app) = app("asset");

        let response = request(app, "/out/not-rison/index.mpd").await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn unknown_track_returns_not_found() {
        let (_dir, app) = app("asset");

        let response = request(app, "/out/(asset:asset)/missing.m3u8").await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    async fn request(app: Router, uri: &str) -> Response {
        app.oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    async fn body(response: Response) -> String {
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    fn app(asset: &str) -> (TempDir, Router) {
        let dir = tempfile::tempdir().unwrap();
        let asset_path = dir.path().join(format!("{asset}.json"));
        fs::create_dir_all(asset_path.parent().unwrap()).unwrap();
        fs::copy(
            format!("{FIXTURES}/video_avc_1080.mp4"),
            asset_path.parent().unwrap().join("video_avc_1080.mp4"),
        )
        .unwrap();
        fs::copy(
            format!("{FIXTURES}/audio_aac_nl_2.mp4"),
            asset_path.parent().unwrap().join("audio_aac_nl_2.mp4"),
        )
        .unwrap();
        fs::write(
            asset_path.parent().unwrap().join("subtitles_nld.vtt"),
            "WEBVTT\n\n00:00.000 --> 00:02.000\nHello\n",
        )
        .unwrap();
        fs::write(asset_path, asset_json()).unwrap();
        let op = Operator::new(Fs::default().root(dir.path().to_str().unwrap())).unwrap();
        (dir, build_router(op))
    }

    fn asset_json() -> &'static str {
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
            },
            {
              "id": "text-nld",
              "path": "subtitles_nld.vtt",
              "codec": "wvtt",
              "type": "text",
              "language": "nld"
            }
          ]
        }"#
    }
}
