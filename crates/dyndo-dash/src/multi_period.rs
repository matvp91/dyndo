use std::ops::Range;
use std::time::Duration;

use dash_mpd::{
    AdaptationSet, MPD, Period, S, SegmentTemplate, SegmentTimeline, SupplementalProperty,
};
use dyndo_core::time::Time;

use crate::DashError;

const PERIOD_CONTINUITY_SCHEME: &str = "urn:mpeg:dash:period-continuity:2015";
const PERIOD_CONNECTIVITY_SCHEME: &str = "urn:mpeg:dash:period-connectivity:2015";

/// Splits dyndo's un-compacted single-period MPD at presentation-time boundaries.
///
/// The timelines retain their original media timestamps, which keeps `$Time$`
/// segment addresses stable. `$Number$` templates advance their start number
/// by the number of preceding segments. A period's presentation-time offset
/// moves by its millisecond boundary in each template's timescale.
pub(crate) fn split(mpd: &mut MPD, boundaries: &[u32]) -> Result<(), DashError> {
    let duration = presentation_duration(mpd)?;
    let spans = period_spans(boundaries, duration);
    let source = single_period(mpd)?;
    let mut periods = Vec::with_capacity(spans.len());

    for (index, span) in spans.iter().enumerate() {
        let previous = periods.last();
        if let Some(period) = split_period(index, &source, span, previous)? {
            periods.push(period);
        }
    }

    mpd.periods = periods;
    Ok(())
}

fn presentation_duration(mpd: &MPD) -> Result<u32, DashError> {
    let Some(duration) = mpd.mediaPresentationDuration else {
        return Err(DashError::MultiPeriodDuration);
    };

    u32::try_from(duration.as_millis()).map_err(|_| DashError::MultiPeriodDuration)
}

fn single_period(mpd: &mut MPD) -> Result<Period, DashError> {
    match mpd.periods.len() {
        0 => Ok(Period::default()),
        1 => mpd.periods.pop().ok_or(DashError::MultiPeriodSource),
        _ => Err(DashError::MultiPeriodSource),
    }
}

fn split_period(
    index: usize,
    source: &Period,
    span: &Range<u32>,
    previous: Option<&Period>,
) -> Result<Option<Period>, DashError> {
    let adaptations: Vec<AdaptationSet> = source
        .adaptations
        .iter()
        .map(|adaptation| split_adaptation_set(adaptation, span, previous))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect();
    if adaptations.is_empty() {
        return Ok(None);
    }

    Ok(Some(Period {
        id: Some(index.to_string()),
        start: Some(Duration::from_millis(u64::from(span.start))),
        duration: Some(Duration::from_millis(u64::from(span.end - span.start))),
        adaptations,
        ..source.clone()
    }))
}

fn split_adaptation_set(
    source: &AdaptationSet,
    span: &Range<u32>,
    previous: Option<&Period>,
) -> Result<Option<AdaptationSet>, DashError> {
    let representations: Vec<dash_mpd::Representation> = source
        .representations
        .iter()
        .map(|representation| split_representation(representation, span))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect();
    if representations.is_empty() {
        return Ok(None);
    }

    let mut adaptation = source.clone();
    adaptation.representations = representations;
    adaptation.supplemental_property = period_relationship(&adaptation, previous)?;
    Ok(Some(adaptation))
}

fn split_representation(
    source: &dash_mpd::Representation,
    span: &Range<u32>,
) -> Result<Option<dash_mpd::Representation>, DashError> {
    let Some(template) = source.SegmentTemplate.as_ref() else {
        return Err(DashError::MultiPeriodTemplate);
    };
    let mut representation = source.clone();
    let template = split_template(template, span)?;
    if template
        .SegmentTimeline
        .as_ref()
        .is_none_or(|timeline| timeline.segments.is_empty())
    {
        return Ok(None);
    }
    representation.SegmentTemplate = Some(template);
    Ok(Some(representation))
}

fn split_template(
    source: &SegmentTemplate,
    span: &Range<u32>,
) -> Result<SegmentTemplate, DashError> {
    let (Some(timescale), Some(timeline)) = (source.timescale, source.SegmentTimeline.as_ref())
    else {
        return Err(DashError::MultiPeriodTemplate);
    };
    let timescale = u32::try_from(timescale).map_err(|_| DashError::MultiPeriodTemplate)?;
    if timescale == 0 {
        return Err(DashError::MultiPeriodTemplate);
    }

    let mut template = source.clone();
    template.presentationTimeOffset = Some(slid_presentation_time_offset(source, span.start));
    let (timeline, first_segment) = slice_timeline(timeline, timescale, span)?;
    if source
        .media
        .as_deref()
        .is_some_and(|media| media.contains("$Number"))
    {
        template.startNumber = Some(
            source
                .startNumber
                .unwrap_or(1)
                .saturating_add(first_segment),
        );
    }
    template.SegmentTimeline = Some(timeline);
    Ok(template)
}

fn slid_presentation_time_offset(template: &SegmentTemplate, boundary_ms: u32) -> u64 {
    let timescale = template.timescale.unwrap_or_default();
    let offset = u128::from(boundary_ms) * u128::from(timescale) / 1_000;
    template
        .presentationTimeOffset
        .unwrap_or_default()
        .saturating_add(u64::try_from(offset).unwrap_or(u64::MAX))
}

fn slice_timeline(
    timeline: &SegmentTimeline,
    timescale: u32,
    span: &Range<u32>,
) -> Result<(SegmentTimeline, u64), DashError> {
    let mut segments = Vec::new();
    let mut next_start = None;
    let mut previous_end = None;
    let mut segment_number = 0_u64;
    let mut first_segment = None;

    for entry in &timeline.segments {
        let Some(start) = entry.t.or(next_start) else {
            return Err(DashError::MultiPeriodTimeline);
        };
        let repeats = u64::try_from(entry.r.unwrap_or_default())
            .map_err(|_| DashError::MultiPeriodTimeline)?;
        let mut segment_start = start;

        for _ in 0..=repeats {
            let segment_end = segment_start
                .checked_add(entry.d)
                .ok_or(DashError::MultiPeriodTimeline)?;
            if belongs_to_period(segment_start, segment_end, timescale, span) {
                first_segment.get_or_insert(segment_number);
                append_segment(&mut segments, &mut previous_end, segment_start, entry.d);
            }
            segment_start = segment_end;
            segment_number = segment_number.saturating_add(1);
        }
        next_start = Some(segment_start);
    }

    Ok((
        SegmentTimeline { segments },
        first_segment.unwrap_or(segment_number),
    ))
}

fn belongs_to_period(start: u64, end: u64, timescale: u32, span: &Range<u32>) -> bool {
    let start_ms = Time::milliseconds(start, timescale);
    let end_ms = Time::milliseconds(end, timescale);
    start_ms < u64::from(span.end) && u64::from(span.start) < end_ms
}

fn append_segment(
    segments: &mut Vec<S>,
    previous_end: &mut Option<u64>,
    start: u64,
    duration: u64,
) {
    let continues = *previous_end == Some(start);
    match segments.last_mut() {
        Some(previous) if previous.d == duration && continues => {
            *previous.r.get_or_insert(0) += 1;
        }
        _ => segments.push(S {
            t: (!continues).then_some(start),
            d: duration,
            ..Default::default()
        }),
    }
    *previous_end = start.checked_add(duration);
}

fn period_relationship(
    adaptation: &AdaptationSet,
    previous: Option<&Period>,
) -> Result<Vec<SupplementalProperty>, DashError> {
    let Some(id) = adaptation.id.as_deref() else {
        return Ok(Vec::new());
    };
    let Some(previous) = previous.filter(|period| {
        period
            .adaptations
            .iter()
            .any(|candidate| candidate.id.as_deref() == Some(id))
    }) else {
        return Ok(Vec::new());
    };

    let scheme_id_uri = if is_continuous(adaptation, previous)? {
        PERIOD_CONTINUITY_SCHEME
    } else {
        PERIOD_CONNECTIVITY_SCHEME
    };
    Ok(vec![SupplementalProperty {
        schemeIdUri: scheme_id_uri.to_string(),
        value: previous.id.clone(),
        ..Default::default()
    }])
}

fn is_continuous(adaptation: &AdaptationSet, previous: &Period) -> Result<bool, DashError> {
    let Some(previous_adaptation) = previous
        .adaptations
        .iter()
        .find(|candidate| candidate.id == adaptation.id)
    else {
        return Ok(false);
    };

    for current in &adaptation.representations {
        let Some(previous) = previous_adaptation
            .representations
            .iter()
            .find(|candidate| candidate.id == current.id)
        else {
            return Ok(false);
        };
        let (Some(current), Some(previous)) = (
            current.SegmentTemplate.as_ref(),
            previous.SegmentTemplate.as_ref(),
        ) else {
            return Ok(false);
        };
        let Some(boundary) = current.presentationTimeOffset else {
            return Ok(false);
        };

        if !timeline_has_start(current, boundary)? || !timeline_has_end(previous, boundary)? {
            return Ok(false);
        }
    }

    Ok(true)
}

fn timeline_has_start(template: &SegmentTemplate, timestamp: u64) -> Result<bool, DashError> {
    timeline_has_timestamp(template, |start, _| start == timestamp)
}

fn timeline_has_end(template: &SegmentTemplate, timestamp: u64) -> Result<bool, DashError> {
    timeline_has_timestamp(template, |_, end| end == timestamp)
}

fn timeline_has_timestamp(
    template: &SegmentTemplate,
    matches: impl Fn(u64, u64) -> bool,
) -> Result<bool, DashError> {
    let Some(timeline) = template.SegmentTimeline.as_ref() else {
        return Err(DashError::MultiPeriodTemplate);
    };
    let mut next_start = None;

    for entry in &timeline.segments {
        let Some(start) = entry.t.or(next_start) else {
            return Err(DashError::MultiPeriodTimeline);
        };
        let repeats = u64::try_from(entry.r.unwrap_or_default())
            .map_err(|_| DashError::MultiPeriodTimeline)?;
        let mut segment_start = start;

        for _ in 0..=repeats {
            let segment_end = segment_start
                .checked_add(entry.d)
                .ok_or(DashError::MultiPeriodTimeline)?;
            if matches(segment_start, segment_end) {
                return Ok(true);
            }
            segment_start = segment_end;
        }
        next_start = Some(segment_start);
    }

    Ok(false)
}

fn period_spans(boundaries: &[u32], duration: u32) -> Vec<Range<u32>> {
    let mut edges: Vec<_> = boundaries
        .iter()
        .copied()
        .filter(|boundary| 0 < *boundary && *boundary < duration)
        .collect();
    edges.sort_unstable();
    edges.dedup();

    let mut spans = Vec::with_capacity(edges.len() + 1);
    let mut start = 0;
    for end in edges.into_iter().chain([duration]) {
        spans.push(start..end);
        start = end;
    }
    spans
}
