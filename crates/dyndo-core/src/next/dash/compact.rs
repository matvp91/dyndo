//! Lossless MPD compaction through DASH descriptor inheritance.

use dash_mpd::{AdaptationSet, MPD, SegmentTemplate};

pub(super) fn compact(mpd: &mut MPD) {
    for period in &mut mpd.periods {
        for set in &mut period.adaptations {
            hoist_identical_template(set);
            hoist_common_template_fields(set);
        }
    }
}

fn hoist_identical_template(set: &mut AdaptationSet) {
    if set.SegmentTemplate.is_some() || set.representations.is_empty() {
        return;
    }
    let Some(first) = set.representations[0].SegmentTemplate.clone() else {
        return;
    };
    if set
        .representations
        .iter()
        .all(|representation| representation.SegmentTemplate.as_ref() == Some(&first))
    {
        set.SegmentTemplate = Some(first);
        for representation in &mut set.representations {
            representation.SegmentTemplate = None;
        }
    }
}

fn hoist_common_template_fields(set: &mut AdaptationSet) {
    if set.SegmentTemplate.is_some()
        || set.representations.len() < 2
        || set
            .representations
            .iter()
            .any(|representation| representation.SegmentTemplate.is_none())
    {
        return;
    }

    let mut shared = SegmentTemplate::default();
    let mut hoisted = false;
    macro_rules! hoist {
        ($field:ident) => {{
            let first = set.representations[0]
                .SegmentTemplate
                .as_ref()
                .and_then(|template| template.$field.clone());
            if first.is_some()
                && set.representations.iter().all(|representation| {
                    representation
                        .SegmentTemplate
                        .as_ref()
                        .and_then(|template| template.$field.clone())
                        == first
                })
            {
                shared.$field = first;
                for representation in &mut set.representations {
                    if let Some(template) = &mut representation.SegmentTemplate {
                        template.$field = None;
                    }
                }
                hoisted = true;
            }
        }};
    }

    hoist!(media);
    hoist!(initialization);
    hoist!(timescale);
    hoist!(presentationTimeOffset);
    hoist!(SegmentTimeline);

    if hoisted {
        set.SegmentTemplate = Some(shared);
        for representation in &mut set.representations {
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

    fn representation(media: &str) -> Representation {
        Representation {
            SegmentTemplate: Some(SegmentTemplate {
                media: Some(media.to_string()),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn identical_templates_are_inherited_from_the_adaptation_set() {
        let mut set = AdaptationSet {
            representations: vec![
                representation("segment-$Time$"),
                representation("segment-$Time$"),
            ],
            ..Default::default()
        };

        hoist_identical_template(&mut set);

        assert!(set.SegmentTemplate.is_some());
        assert!(
            set.representations
                .iter()
                .all(|item| item.SegmentTemplate.is_none())
        );
    }

    #[test]
    fn different_templates_remain_on_the_representations() {
        let mut set = AdaptationSet {
            representations: vec![representation("a"), representation("b")],
            ..Default::default()
        };

        hoist_identical_template(&mut set);

        assert!(set.SegmentTemplate.is_none());
    }
}
