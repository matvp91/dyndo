use mp4_atom::{
    BufMut, Codec, Dinf, Dref, Encode, FourCC, Ftyp, Hdlr, Mdhd, Mdia, Minf, Moov, Mvex, Mvhd,
    Nmhd, SegmentReference, Sidx, Stbl, Stco, Stsd, Styp, Tkhd, Trak, Trex, Url,
};

use super::{MediaSegment, PackagerError};

/// Defines how a payload type is written as a CMAF track.
pub trait Format {
    /// The payload carried by each media sample.
    type Payload;

    /// Returns the file type box for the packaged track.
    fn file_type(&self) -> Ftyp;

    /// Returns the segment type box for each media segment.
    fn segment_type(&self) -> Styp;

    /// Returns the track handler type.
    fn handler(&self) -> FourCC;

    /// Returns the sample entry for the track initialization segment.
    fn sample_entry(&self) -> Codec;

    /// Writes one payload into the media data box.
    fn write_sample<B: BufMut>(
        &self,
        payload: &Self::Payload,
        output: &mut B,
    ) -> mp4_atom::Result<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Packages media segments using a [`Format`] implementation.
pub struct Packager<F> {
    format: F,
    track_id: u32,
    timescale: u32,
}

impl<F: Format> Packager<F> {
    /// Creates a packager with track ID 1.
    pub fn new(format: F, timescale: u32) -> Self {
        Self {
            format,
            track_id: 1,
            timescale,
        }
    }

    /// Sets the track ID written to the packaged CMAF track.
    pub fn with_track_id(mut self, track_id: u32) -> Self {
        self.track_id = track_id;
        self
    }

    /// Returns the configured track ID.
    pub fn track_id(&self) -> u32 {
        self.track_id
    }

    /// Returns the media timescale.
    pub fn timescale(&self) -> u32 {
        self.timescale
    }

    /// Packages media segments into a complete CMAF track.
    ///
    /// # Errors
    ///
    /// Returns [`PackagerError`] if the configuration or media segments cannot
    /// be represented as a CMAF track.
    pub fn package(&self, segments: &[MediaSegment<F::Payload>]) -> Result<Vec<u8>, PackagerError> {
        if self.track_id == 0 {
            return Err(PackagerError::InvalidTrackId);
        }
        if self.timescale == 0 {
            return Err(PackagerError::InvalidTimescale);
        }

        let duration = segments
            .last()
            .map(|segment| {
                segment
                    .base_decode_time()
                    .saturating_add(segment.duration())
            })
            .unwrap_or(0);
        if duration == 0 {
            return Err(PackagerError::Empty);
        }

        let mut serialized_segments = Vec::with_capacity(segments.len());
        let mut references = Vec::with_capacity(segments.len());
        for (index, segment) in segments.iter().enumerate() {
            let sequence_number = index
                .checked_add(1)
                .and_then(|index| u32::try_from(index).ok())
                .ok_or(PackagerError::TooManyMediaSegments)?;
            let bytes = segment.serialize(&self.format, self.track_id, sequence_number)?;
            let size =
                u32::try_from(bytes.len()).map_err(|_| PackagerError::MediaSegmentTooLarge)?;
            references.push(reference(size, segment)?);
            serialized_segments.push(bytes);
        }

        let mut bytes = Vec::new();
        self.format.file_type().encode(&mut bytes)?;
        movie(&self.format, self.track_id, self.timescale, duration).encode(&mut bytes)?;
        Sidx {
            reference_id: self.track_id,
            timescale: self.timescale,
            earliest_presentation_time: segments
                .first()
                .map_or(0, |segment| segment.base_decode_time()),
            first_offset: 0,
            references,
        }
        .encode(&mut bytes)?;
        for segment in serialized_segments {
            bytes.extend_from_slice(&segment);
        }

        Ok(bytes)
    }
}

fn reference<P>(size: u32, segment: &MediaSegment<P>) -> Result<SegmentReference, PackagerError> {
    Ok(SegmentReference {
        reference_type: false,
        reference_size: size,
        subsegment_duration: u32::try_from(segment.duration())
            .map_err(|_| PackagerError::DurationOverflow)?,
        starts_with_sap: true,
        sap_type: 1,
        sap_delta_time: 0,
    })
}

fn movie<F: Format>(format: &F, track_id: u32, timescale: u32, duration: u64) -> Moov {
    Moov {
        mvhd: Mvhd {
            creation_time: 0,
            modification_time: 0,
            timescale,
            duration,
            rate: 1.into(),
            volume: 1.into(),
            matrix: Default::default(),
            next_track_id: track_id.saturating_add(1),
        },
        mvex: Some(Mvex {
            mehd: None,
            trex: vec![Trex {
                track_id,
                default_sample_description_index: 1,
                ..Trex::default()
            }],
        }),
        trak: vec![Trak {
            tkhd: Tkhd {
                track_id,
                duration,
                enabled: true,
                in_movie: true,
                ..Tkhd::default()
            },
            mdia: Mdia {
                mdhd: Mdhd {
                    timescale,
                    duration,
                    language: "und".to_string(),
                    ..Mdhd::default()
                },
                hdlr: Hdlr {
                    handler: format.handler(),
                    name: "dyndo".to_string(),
                },
                minf: Minf {
                    nmhd: Some(Nmhd {}),
                    dinf: Dinf {
                        dref: Dref {
                            urls: vec![Url {
                                location: String::new(),
                            }],
                        },
                    },
                    stbl: Stbl {
                        stsd: Stsd {
                            codecs: vec![format.sample_entry()],
                        },
                        stco: Some(Stco::default()),
                        ..Stbl::default()
                    },
                    ..Minf::default()
                },
            },
            ..Trak::default()
        }],
        ..Moov::default()
    }
}
