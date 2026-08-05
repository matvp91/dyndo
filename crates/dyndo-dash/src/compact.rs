//! Compacts a built MPD by hoisting shared segment-template data.
//!
//! DASH representations inherit `SegmentTemplate` fields from their parent
//! `AdaptationSet`, so moving identical fields upward preserves the manifest's
//! meaning while reducing the serialized XML.

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

#[cfg(test)]
mod tests {
    use dash_mpd::Representation;

    use super::*;

    fn representation_with_media(media: &str) -> Representation {
        Representation {
            SegmentTemplate: Some(SegmentTemplate {
                media: Some(media.to_string()),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn shared_template_is_hoisted_when_templates_are_identical() {
        let mut adaptation_set = AdaptationSet {
            representations: vec![
                representation_with_media("x"),
                representation_with_media("x"),
            ],
            ..Default::default()
        };

        hoist_shared_template(&mut adaptation_set);

        assert!(adaptation_set.SegmentTemplate.is_some());
        assert!(
            adaptation_set
                .representations
                .iter()
                .all(|representation| representation.SegmentTemplate.is_none())
        );
    }

    #[test]
    fn shared_template_stays_on_representations_when_templates_differ() {
        let mut adaptation_set = AdaptationSet {
            representations: vec![
                representation_with_media("x"),
                representation_with_media("y"),
            ],
            ..Default::default()
        };

        hoist_shared_template(&mut adaptation_set);

        assert!(adaptation_set.SegmentTemplate.is_none());
    }

    #[test]
    fn shared_attributes_are_hoisted_when_templates_differ() {
        let mut first = representation_with_media("same");
        first.SegmentTemplate.as_mut().unwrap().timescale = Some(1_000);
        let mut second = representation_with_media("same");
        second.SegmentTemplate.as_mut().unwrap().timescale = Some(48_000);
        let mut adaptation_set = AdaptationSet {
            representations: vec![first, second],
            ..Default::default()
        };

        hoist_shared_attributes(&mut adaptation_set);

        assert_eq!(
            adaptation_set
                .SegmentTemplate
                .and_then(|template| template.media),
            Some("same".to_string())
        );
    }
}
