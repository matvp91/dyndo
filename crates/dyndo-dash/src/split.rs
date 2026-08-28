use std::{ops::Range, time::Duration};

use dash_mpd::{
    AdaptationSet, MPD, Period, S, SegmentTemplate, SegmentTimeline, SupplementalProperty,
};

use crate::DashError;

const PERIOD_CONTINUITY_SCHEME: &str = "urn:mpeg:dash:period-continuity:2015";
const PERIOD_CONNECTIVITY_SCHEME: &str = "urn:mpeg:dash:period-connectivity:2015";

pub(crate) fn split(mut mpd: MPD, boundaries: &[Duration]) -> Result<MPD, DashError> {
    let duration = mpd
        .mediaPresentationDuration
        .ok_or(DashError::MultiPeriodDuration)?;
    let spans = period_spans(boundaries, duration);
    let source = single_period(&mut mpd)?;
    let mut periods = Vec::with_capacity(spans.len());

    for (index, span) in spans.iter().enumerate() {
        if let Some(period) = split_period(index, &source, span, periods.last())? {
            periods.push(period);
        }
    }

    mpd.periods = periods;
    Ok(mpd)
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
    span: &Range<Duration>,
    previous: Option<&Period>,
) -> Result<Option<Period>, DashError> {
    let adaptations = source
        .adaptations
        .iter()
        .map(|adaptation| split_adaptation_set(adaptation, span, previous))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    if adaptations.is_empty() {
        return Ok(None);
    }

    Ok(Some(Period {
        id: Some(index.to_string()),
        start: Some(span.start),
        duration: Some(span.end.saturating_sub(span.start)),
        adaptations,
        ..source.clone()
    }))
}

fn split_adaptation_set(
    source: &AdaptationSet,
    span: &Range<Duration>,
    previous: Option<&Period>,
) -> Result<Option<AdaptationSet>, DashError> {
    let representations = source
        .representations
        .iter()
        .map(|representation| split_representation(representation, span))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
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
    span: &Range<Duration>,
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
    span: &Range<Duration>,
) -> Result<SegmentTemplate, DashError> {
    let (Some(timescale), Some(timeline)) = (source.timescale, source.SegmentTimeline.as_ref())
    else {
        return Err(DashError::MultiPeriodTemplate);
    };
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

fn slid_presentation_time_offset(template: &SegmentTemplate, boundary: Duration) -> u64 {
    template
        .presentationTimeOffset
        .unwrap_or_default()
        .saturating_add(duration_ticks(
            boundary,
            template.timescale.unwrap_or_default(),
        ))
}

fn slice_timeline(
    timeline: &SegmentTimeline,
    timescale: u64,
    span: &Range<Duration>,
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

fn belongs_to_period(start: u64, end: u64, timescale: u64, span: &Range<Duration>) -> bool {
    ticks_duration(start, timescale) < span.end && span.start < ticks_duration(end, timescale)
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

    let scheme = if is_continuous(adaptation, previous)? {
        PERIOD_CONTINUITY_SCHEME
    } else {
        PERIOD_CONNECTIVITY_SCHEME
    };
    Ok(vec![SupplementalProperty {
        schemeIdUri: scheme.into(),
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

fn period_spans(boundaries: &[Duration], duration: Duration) -> Vec<Range<Duration>> {
    let mut edges = boundaries
        .iter()
        .copied()
        .filter(|boundary| Duration::ZERO < *boundary && *boundary < duration)
        .collect::<Vec<_>>();
    edges.sort_unstable();
    edges.dedup();

    let mut spans = Vec::with_capacity(edges.len() + 1);
    let mut start = Duration::ZERO;
    for end in edges.into_iter().chain([duration]) {
        spans.push(start..end);
        start = end;
    }
    spans
}

fn duration_ticks(duration: Duration, timescale: u64) -> u64 {
    let ticks = duration.as_nanos().saturating_mul(u128::from(timescale)) / 1_000_000_000;
    u64::try_from(ticks).unwrap_or(u64::MAX)
}

fn ticks_duration(ticks: u64, timescale: u64) -> Duration {
    if timescale == 0 {
        return Duration::MAX;
    }
    Duration::from_secs(ticks / timescale)
        + Duration::from_nanos(ticks % timescale * 1_000_000_000 / timescale)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use dash_mpd::{
        AdaptationSet, MPD, Period, Representation, S, SegmentTemplate, SegmentTimeline,
    };

    use super::split;

    fn mpd() -> MPD {
        MPD {
            mediaPresentationDuration: Some(Duration::from_secs(2)),
            periods: vec![Period {
                adaptations: vec![AdaptationSet {
                    id: Some("0".into()),
                    representations: vec![Representation {
                        id: Some("video".into()),
                        SegmentTemplate: Some(SegmentTemplate {
                            media: Some("$RepresentationID$/$Number$.m4s".into()),
                            startNumber: Some(1),
                            timescale: Some(1_000),
                            presentationTimeOffset: Some(0),
                            SegmentTimeline: Some(SegmentTimeline {
                                segments: vec![S {
                                    t: Some(0),
                                    d: 1_000,
                                    r: Some(1),
                                    ..Default::default()
                                }],
                            }),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }],
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn split_creates_a_period_for_each_boundary_span() {
        let mpd = split(mpd(), &[Duration::from_secs(1)]).unwrap();

        assert_eq!(mpd.periods.len(), 2);
    }

    #[test]
    fn split_advances_numbered_templates_in_later_periods() {
        let mpd = split(mpd(), &[Duration::from_secs(1)]).unwrap();
        let template = mpd.periods[1].adaptations[0].representations[0]
            .SegmentTemplate
            .as_ref();

        assert_eq!(template.and_then(|template| template.startNumber), Some(2));
    }
}
