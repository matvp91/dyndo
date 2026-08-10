mod atom;
mod fragment;
mod track;

use mp4_atom::{Any, DecodeMaybe, Encode, Moof, Sidx};

use super::TimedTrack;

const TRACK_ID: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WvttSample {
    cues: Vec<String>,
}

impl WvttSample {
    pub fn new(cues: Vec<String>) -> Self {
        Self { cues }
    }

    pub fn cues(&self) -> &[String] {
        &self.cues
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PackageError {
    #[error("track covers no time")]
    Empty,
    #[error("a sample is too large")]
    SampleTooLarge,
    #[error("a fragment is too large")]
    FragmentTooLarge,
    #[error("a fragment duration overflows")]
    DurationOverflow,
    #[error("track contains too many fragments")]
    TooManyFragments,
    #[error(transparent)]
    Atom(#[from] mp4_atom::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum UnpackageError {
    #[error("fragment carries no base decode time")]
    MissingBaseTime,
    #[error("fragment carries no sample durations")]
    MissingSampleTiming,
    #[error("sample data overruns the fragment")]
    SampleOutOfRange,
    #[error("a fragment header and its sample data do not pair up")]
    UnpairedFragment,
    #[error(transparent)]
    Atom(#[from] mp4_atom::Error),
}

pub fn package(track: &TimedTrack<WvttSample>) -> Result<Vec<u8>, PackageError> {
    let duration = track
        .fragments()
        .last()
        .map(|fragment| fragment.start_time().saturating_add(fragment.duration()))
        .unwrap_or(0);
    if duration == 0 {
        return Err(PackageError::Empty);
    }

    let mut encoded = Vec::with_capacity(track.fragments().len());
    let mut references = Vec::with_capacity(track.fragments().len());
    for (index, timed_fragment) in track.fragments().iter().enumerate() {
        let bytes = fragment::encode(index, timed_fragment)?;
        let size = u32::try_from(bytes.len()).map_err(|_| PackageError::FragmentTooLarge)?;
        references.push(track::reference(size, timed_fragment)?);
        encoded.push(bytes);
    }

    let mut bytes = Vec::new();
    track::ftyp().encode(&mut bytes)?;
    track::moov(track.timescale(), duration).encode(&mut bytes)?;
    Sidx {
        reference_id: TRACK_ID,
        timescale: track.timescale(),
        earliest_presentation_time: track
            .fragments()
            .first()
            .map_or(0, |fragment| fragment.start_time()),
        first_offset: 0,
        references,
    }
    .encode(&mut bytes)?;
    for timed_fragment in encoded {
        bytes.extend_from_slice(&timed_fragment);
    }

    Ok(bytes)
}

pub fn unpackage(segment: &[u8], timescale: u32) -> Result<TimedTrack<WvttSample>, UnpackageError> {
    let mut fragments = Vec::new();
    let mut header: Option<Moof> = None;
    let mut buf = segment;

    while let Some(atom) = Any::decode_maybe(&mut buf)? {
        match atom {
            Any::Moof(moof) => {
                if header.replace(moof).is_some() {
                    return Err(UnpackageError::UnpairedFragment);
                }
            }
            Any::Mdat(mdat) => {
                let header = header.take().ok_or(UnpackageError::UnpairedFragment)?;
                fragments.push(fragment::decode(&header, &mdat.data)?);
            }
            _ => {}
        }
    }

    if header.is_some() {
        return Err(UnpackageError::UnpairedFragment);
    }

    Ok(TimedTrack::new(timescale, fragments))
}
