use dash_mpd::{AdaptationSet, MPD, SegmentTemplate};

pub fn compact(mpd: &mut MPD) {
    for period in &mut mpd.periods {
        for adaptation_set in &mut period.adaptations {
            hoist_shared_template(adaptation_set);
            hoist_shared_attributes(adaptation_set);
        }
    }
}

fn hoist_shared_template(adaptation_set: &mut AdaptationSet) {
    if adaptation_set.SegmentTemplate.is_some() || adaptation_set.representations.is_empty() {
        return;
    }
    let Some(first) = adaptation_set.representations[0].SegmentTemplate.clone() else {
        return;
    };
    if !adaptation_set
        .representations
        .iter()
        .all(|representation| representation.SegmentTemplate.as_ref() == Some(&first))
    {
        return;
    }

    adaptation_set.SegmentTemplate = Some(first);
    for representation in &mut adaptation_set.representations {
        representation.SegmentTemplate = None;
    }
}

fn hoist_shared_attributes(adaptation_set: &mut AdaptationSet) {
    if adaptation_set.SegmentTemplate.is_some() || adaptation_set.representations.len() < 2 {
        return;
    }
    if !adaptation_set
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
            let first = adaptation_set.representations[0]
                .SegmentTemplate
                .as_ref()
                .expect("all representations were checked to have a SegmentTemplate")
                .$field
                .clone();
            if first.is_some()
                && adaptation_set.representations.iter().all(|representation| {
                    representation
                        .SegmentTemplate
                        .as_ref()
                        .expect("all representations were checked to have a SegmentTemplate")
                        .$field
                        == first
                })
            {
                shared.$field = first;
                for representation in &mut adaptation_set.representations {
                    representation
                        .SegmentTemplate
                        .as_mut()
                        .expect("all representations were checked to have a SegmentTemplate")
                        .$field = None;
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
        adaptation_set.SegmentTemplate = Some(shared);
        for representation in &mut adaptation_set.representations {
            if representation.SegmentTemplate.as_ref() == Some(&SegmentTemplate::default()) {
                representation.SegmentTemplate = None;
            }
        }
    }
}
