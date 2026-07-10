use std::io::Cursor;

use mp4_atom::{Atom, Header, Moof, Moov, ReadAtom, ReadFrom, Sidx};

use crate::error::{Error, Result};
use crate::storage::Source;

/// A single box's parsed header plus the absolute byte range of its body.
/// The body spans `[body_start, box_end)`; `body_len()` is that range's length
/// (equivalently `header.size`, which excludes the header per ISO-BMFF).
struct BoxSpan {
    header: Header,
    body_start: u64,
    box_end: u64,
}

impl BoxSpan {
    fn body_len(&self) -> usize {
        (self.box_end - self.body_start) as usize
    }
}

/// Read and parse the box header at `offset`, computing its byte span.
/// Returns `None` at end-of-source (fewer than 8 bytes remain).
async fn read_box_span<S: Source>(source: &S, offset: u64, path: &str) -> Result<Option<BoxSpan>> {
    let head_bytes = source.read_at(offset, 16).await?;
    if head_bytes.len() < 8 {
        return Ok(None); // reached end without a full box header
    }
    let mut cursor = Cursor::new(&head_bytes[..]);
    let header = Header::read_from(&mut cursor).map_err(|e| Error::MalformedBox {
        box_type: "box header".into(),
        path: path.into(),
        reason: e.to_string(),
    })?;
    let header_len = cursor.position();
    let body_len = header.size.ok_or_else(|| Error::MalformedBox {
        box_type: "box".into(),
        path: path.into(),
        reason: "unbounded box size".into(),
    })?;
    let body_start = offset
        .checked_add(header_len)
        .ok_or_else(|| Error::MalformedBox {
            box_type: "box".into(),
            path: path.into(),
            reason: "box size overflow".into(),
        })?;
    let box_end = body_start
        .checked_add(body_len as u64)
        .ok_or_else(|| Error::MalformedBox {
            box_type: "box".into(),
            path: path.into(),
            reason: "box size overflow".into(),
        })?;
    Ok(Some(BoxSpan {
        header,
        body_start,
        box_end,
    }))
}

/// Fetch a box body and decode it into atom `A`.
async fn read_atom<A: ReadAtom, S: Source>(
    source: &S,
    span: &BoxSpan,
    name: &str,
    path: &str,
) -> Result<A> {
    let body = source.read_at(span.body_start, span.body_len()).await?;
    A::read_atom(&span.header, &mut Cursor::new(&body[..])).map_err(|e| Error::MalformedBox {
        box_type: name.into(),
        path: path.into(),
        reason: e.to_string(),
    })
}

/// The header boxes we care about, with the byte offsets they end at.
pub(super) struct HeaderBoxes {
    pub(super) moov: Moov,
    pub(super) sidx: Sidx,
    pub(super) first_moof: Moof,
    pub(super) moov_end: u64,
    pub(super) sidx_end: u64,
}

/// Header-first scan: read moov, sidx and the first moof; skip everything else
/// (notably mdat, which we never fetch). Stops as soon as all three are seen.
/// The first moof supplies the video sample duration used to derive frame rate.
pub(super) async fn scan_header_boxes<S: Source>(source: &S, path: &str) -> Result<HeaderBoxes> {
    let mut offset = 0u64;
    let mut moov: Option<Moov> = None;
    let mut sidx: Option<Sidx> = None;
    let mut first_moof: Option<Moof> = None;
    let mut moov_end = 0u64;
    let mut sidx_end = 0u64;

    while moov.is_none() || sidx.is_none() || first_moof.is_none() {
        let Some(span) = read_box_span(source, offset, path).await? else {
            break; // reached end without the boxes we need
        };
        if span.header.kind == Moov::KIND {
            moov = Some(read_atom(source, &span, "moov", path).await?);
            moov_end = span.box_end;
        } else if span.header.kind == Sidx::KIND {
            sidx = Some(read_atom(source, &span, "sidx", path).await?);
            sidx_end = span.box_end;
        } else if span.header.kind == Moof::KIND && first_moof.is_none() {
            first_moof = Some(read_atom(source, &span, "moof", path).await?);
        }
        offset = span.box_end;
    }

    Ok(HeaderBoxes {
        moov: moov.ok_or_else(|| Error::MissingMoov(path.into()))?,
        sidx: sidx.ok_or_else(|| Error::MissingSidx(path.into()))?,
        first_moof: first_moof.ok_or_else(|| Error::MalformedBox {
            box_type: "moof".into(),
            path: path.into(),
            reason: "missing first moof".into(),
        })?,
        moov_end,
        sidx_end,
    })
}
