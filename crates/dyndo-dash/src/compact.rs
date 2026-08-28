use dash_mpd::{AdaptationSet, MPD, SegmentTemplate};

pub(crate) fn compact(mut mpd: MPD) -> MPD {
    for period in &mut mpd.periods {
        for adaptation in &mut period.adaptations {
            hoist_shared_template(adaptation);
            hoist_shared_attributes(adaptation);
        }
    }

    mpd
}

fn hoist_shared_template(adaptation: &mut AdaptationSet) {
    if adaptation.SegmentTemplate.is_some() || adaptation.representations.is_empty() {
        return;
    }
    let Some(first) = adaptation.representations[0].SegmentTemplate.clone() else {
        return;
    };
    if !adaptation
        .representations
        .iter()
        .all(|representation| representation.SegmentTemplate.as_ref() == Some(&first))
    {
        return;
    }

    adaptation.SegmentTemplate = Some(first);
    for representation in &mut adaptation.representations {
        representation.SegmentTemplate = None;
    }
}

fn hoist_shared_attributes(adaptation: &mut AdaptationSet) {
    if adaptation.SegmentTemplate.is_some() || adaptation.representations.len() < 2 {
        return;
    }
    if !adaptation
        .representations
        .iter()
        .all(|representation| representation.SegmentTemplate.is_some())
    {
        return;
    }

    let mut shared = SegmentTemplate::default();
    let mut hoisted_any = false;

    macro_rules! hoist_field {
        ($field:ident) => {{
            let first = adaptation.representations[0]
                .SegmentTemplate
                .as_ref()
                .map(|template| template.$field.clone())
                .unwrap_or_default();
            if first.is_some()
                && adaptation.representations.iter().all(|representation| {
                    representation
                        .SegmentTemplate
                        .as_ref()
                        .is_some_and(|template| template.$field == first)
                })
            {
                shared.$field = first;
                for representation in &mut adaptation.representations {
                    if let Some(template) = &mut representation.SegmentTemplate {
                        template.$field = None;
                    }
                }
                hoisted_any = true;
            }
        }};
    }

    hoist_field!(media);
    hoist_field!(index);
    hoist_field!(initialization);
    hoist_field!(bitstreamSwitching);
    hoist_field!(indexRange);
    hoist_field!(indexRangeExact);
    hoist_field!(startNumber);
    hoist_field!(duration);
    hoist_field!(timescale);
    hoist_field!(eptDelta);
    hoist_field!(pbDelta);
    hoist_field!(presentationTimeOffset);
    hoist_field!(availabilityTimeOffset);
    hoist_field!(availabilityTimeComplete);
    hoist_field!(Initialization);
    hoist_field!(representation_index);
    hoist_field!(failover_content);
    hoist_field!(SegmentTimeline);
    hoist_field!(BitstreamSwitching);

    if hoisted_any {
        adaptation.SegmentTemplate = Some(shared);
        for representation in &mut adaptation.representations {
            if representation.SegmentTemplate.as_ref() == Some(&SegmentTemplate::default()) {
                representation.SegmentTemplate = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use dash_mpd::{AdaptationSet, MPD, Period, Representation, SegmentTemplate};

    use super::compact;

    #[test]
    fn compact_hoists_an_identical_segment_template() {
        let template = SegmentTemplate {
            media: Some("$RepresentationID$/$Time$.m4s".into()),
            timescale: Some(1_000),
            ..Default::default()
        };
        let mut mpd = MPD {
            periods: vec![Period {
                adaptations: vec![AdaptationSet {
                    representations: vec![
                        Representation {
                            SegmentTemplate: Some(template.clone()),
                            ..Default::default()
                        },
                        Representation {
                            SegmentTemplate: Some(template),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        };

        mpd = compact(mpd);

        assert!(mpd.periods[0].adaptations[0].SegmentTemplate.is_some());
    }
}
