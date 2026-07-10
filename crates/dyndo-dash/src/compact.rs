use dash_mpd::{AdaptationSet, SegmentTemplate, MPD};

pub(crate) fn compact(mpd: &mut MPD) {
    for period in &mut mpd.periods {
        for set in &mut period.adaptations {
            hoist_shared_template(set);
            hoist_shared_attributes(set);
        }
    }
}

fn hoist_shared_template(set: &mut AdaptationSet) {
    if set.SegmentTemplate.is_some() || set.representations.is_empty() {
        return;
    }
    let Some(first) = set.representations[0].SegmentTemplate.clone() else {
        return;
    };
    let all_equal = set
        .representations
        .iter()
        .all(|r| r.SegmentTemplate.as_ref() == Some(&first));
    if !all_equal {
        return;
    }
    set.SegmentTemplate = Some(first);
    for rep in &mut set.representations {
        rep.SegmentTemplate = None;
    }
}

fn hoist_shared_attributes(set: &mut AdaptationSet) {
    if set.SegmentTemplate.is_some() || set.representations.len() < 2 {
        return;
    }
    if !set
        .representations
        .iter()
        .all(|r| r.SegmentTemplate.is_some())
    {
        return;
    }

    let mut shared = SegmentTemplate::default();
    let mut hoisted_any = false;

    // Hoist one field: if every rep has it set to the same value, copy it to `shared`
    // and clear it on each rep. `first` is cloned so no borrow is held while mutating.
    macro_rules! hoist_field {
        ($field:ident) => {{
            let first = set.representations[0]
                .SegmentTemplate
                .as_ref()
                .unwrap()
                .$field
                .clone();
            if first.is_some()
                && set
                    .representations
                    .iter()
                    .all(|r| r.SegmentTemplate.as_ref().unwrap().$field == first)
            {
                shared.$field = first;
                for rep in &mut set.representations {
                    rep.SegmentTemplate.as_mut().unwrap().$field = None;
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
        set.SegmentTemplate = Some(shared);
        // Drop per-Representation templates that hoisting has emptied.
        for rep in &mut set.representations {
            if rep.SegmentTemplate.as_ref() == Some(&SegmentTemplate::default()) {
                rep.SegmentTemplate = None;
            }
        }
    }
}
