use std::sync::Arc;

use dyndo_crypt::cpix_parser::{Cpix, CpixParser};
use futures_util::future::try_join_all;
use opendal::Operator;

use super::Asset;
use crate::track::cmaf::ResolvedCmafTrack;
use crate::track::thumbnail::ResolvedThumbnailTrack;
use crate::track::{CmafRepresentationError, ResolvedTrack, Track, TrackResolveError};

impl Asset {
    /// Resolves every configured track in this asset.
    pub async fn resolve(&self, operator: &Operator) -> Result<ResolvedAsset, AssetResolveError> {
        let cpix = self.resolve_cpix(operator).await?.map(Arc::new);
        let mut tracks = self.resolve_source_tracks(operator).await?;
        let mut thumbnails = Vec::new();

        for thumbnail in self.thumbnail_tracks() {
            let resolved = thumbnail
                .resolve(tracks.iter().filter_map(ResolvedTrack::cmaf))
                .ok_or_else(|| AssetResolveError::MissingThumbnailSource {
                    id: thumbnail.id.clone(),
                })?;
            thumbnails.push(ResolvedTrack::Thumbnail(resolved));
        }
        tracks.extend(thumbnails);

        Ok(ResolvedAsset {
            boundaries: self.boundaries.clone(),
            tracks,
            cpix,
        })
    }

    pub async fn resolve_cpix(
        &self,
        operator: &Operator,
    ) -> Result<Option<Cpix>, AssetResolveError> {
        let Some(path) = self.cpix_path() else {
            return Ok(None);
        };
        let bytes = operator.read(path.as_str()).await?;
        Ok(Some(CpixParser::parse_bytes(&bytes.to_bytes())?))
    }

    /// Resolves one configured track by identifier.
    pub async fn resolve_track(
        &self,
        operator: &Operator,
        id: &str,
    ) -> Result<Option<ResolvedTrack>, AssetResolveError> {
        let Some(track) = self.tracks.iter().find(|track| track.id() == id) else {
            return Ok(None);
        };

        match track {
            Track::Source(source) => {
                let path = self.track_path(source);
                source
                    .resolve(operator, &path)
                    .await
                    .map(Some)
                    .map_err(Into::into)
            }
            Track::Thumbnail(thumbnail) => {
                let sources = self.resolve_source_tracks(operator).await?;
                thumbnail
                    .resolve(sources.iter().filter_map(ResolvedTrack::cmaf))
                    .map(ResolvedTrack::Thumbnail)
                    .map(Some)
                    .ok_or_else(|| AssetResolveError::MissingThumbnailSource {
                        id: thumbnail.id.clone(),
                    })
            }
        }
    }

    async fn resolve_source_tracks(
        &self,
        operator: &Operator,
    ) -> Result<Vec<ResolvedTrack>, TrackResolveError> {
        let resolutions = self.source_tracks().map(|track| {
            let path = self.track_path(track);
            async move { track.resolve(operator, &path).await }
        });
        try_join_all(resolutions).await
    }
}

/// An asset whose configured tracks have been resolved for runtime use.
#[derive(Clone)]
pub struct ResolvedAsset {
    boundaries: Vec<u32>,
    tracks: Vec<ResolvedTrack>,
    cpix: Option<Arc<Cpix>>,
}

impl ResolvedAsset {
    pub fn new(boundaries: Vec<u32>, tracks: Vec<ResolvedTrack>) -> Self {
        Self {
            boundaries,
            tracks,
            cpix: None,
        }
    }

    pub fn boundaries(&self) -> &[u32] {
        &self.boundaries
    }

    pub fn tracks(&self) -> &[ResolvedTrack] {
        &self.tracks
    }

    pub fn cpix(&self) -> Option<&Cpix> {
        self.cpix.as_deref()
    }

    pub fn track(&self, id: &str) -> Option<&ResolvedTrack> {
        self.tracks.iter().find(|track| track.id() == id)
    }

    pub fn thumbnails(&self) -> impl Iterator<Item = &ResolvedThumbnailTrack> {
        self.tracks.iter().filter_map(ResolvedTrack::thumbnail)
    }

    pub fn retain_tracks(&mut self, retain: impl FnMut(&ResolvedTrack) -> bool) {
        self.tracks.retain(retain);
    }

    /// Builds CMAF representations for every source track in the asset.
    pub async fn cmaf_representations(
        &self,
        text_length: u32,
    ) -> Result<Vec<ResolvedCmafTrack>, CmafRepresentationError> {
        let representations = self
            .tracks
            .iter()
            .filter(|track| track.thumbnail().is_none())
            .map(|track| track.cmaf_representation(text_length, &self.boundaries));
        try_join_all(representations).await
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AssetResolveError {
    #[error(transparent)]
    Storage(#[from] opendal::Error),
    #[error(transparent)]
    Cpix(#[from] dyndo_crypt::cpix_parser::Error),
    #[error(transparent)]
    Track(#[from] TrackResolveError),
    #[error("thumbnail track {id} has no suitable video source")]
    MissingThumbnailSource { id: String },
}
