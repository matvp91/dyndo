//! Post-process a built MPD in place, hoisting `SegmentTemplate` content shared by
//! every `Representation` in an `AdaptationSet` up to the set level. This is purely a
//! size optimization: under DASH multi-level inheritance (ISO/IEC 23009-1 §5.3.9.1) a
//! `SegmentTemplate` at a higher level is inherited — per attribute and per element —
//! by all child `Representation`s unless overridden, so the effective per-Representation
//! template is unchanged. `$RepresentationID$` still resolves to each Representation's
//! `@id` at the higher level (§5.3.9.4), which is what makes `@media`/`@initialization`
//! hoistable.
//!
//! Two passes run per `AdaptationSet`:
//! 1. [`hoist_shared_template`] — when every `Representation` carries an identical
//!    `SegmentTemplate`, move one copy to the set and clear the reps.
//! 2. [`hoist_shared_attributes`] — for the residual (reps whose templates differ),
//!    move each field common to all reps up to the set, leaving only differing fields
//!    (typically `SegmentTimeline`) per `Representation`.

use dash_mpd::{AdaptationSet, SegmentTemplate, MPD};

/// Hoist shared `SegmentTemplate` content up to the `AdaptationSet` level across the
/// whole MPD, in place. Idempotent: applying it twice equals applying it once.
pub(crate) fn compact(mpd: &mut MPD) {
    for period in &mut mpd.periods {
        for set in &mut period.adaptations {
            hoist_shared_template(set);
            hoist_shared_attributes(set);
        }
    }
}

/// Method 1: when every `Representation` has an identical `SegmentTemplate`, move one
/// copy to the `AdaptationSet` and clear the per-Representation copies. A no-op if the
/// set already has a template, has no representations, or any rep's template differs
/// (or is absent).
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

/// Method 2: for an `AdaptationSet` whose `Representation`s still carry differing
/// `SegmentTemplate`s, hoist every field common to all of them up to the set, leaving
/// only differing fields (typically `SegmentTimeline`) per `Representation`. A field is
/// hoisted only when all reps set it to the same `Some(value)`. If a rep's template is
/// emptied by this, it is dropped to `None`.
///
/// No-op if the set already has a template (method 1 hoisted the identical case), if
/// there are fewer than two reps, or if any rep lacks a template. The field list mirrors
/// `dash_mpd::SegmentTemplate`.
fn hoist_shared_attributes(set: &mut AdaptationSet) {
    if set.SegmentTemplate.is_some() || set.representations.len() < 2 {
        return;
    }
    if !set.representations.iter().all(|r| r.SegmentTemplate.is_some()) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use dash_mpd::{Period, Representation, SegmentTemplate, SegmentTimeline, S};

    /// A SegmentTemplate shaped like the builder's output: fixed init/media strings,
    /// PTO 0, a timescale, and a SegmentTimeline.
    fn tmpl(timescale: u64, timeline: Vec<S>) -> SegmentTemplate {
        SegmentTemplate {
            timescale: Some(timescale),
            presentationTimeOffset: Some(0),
            initialization: Some("$RepresentationID$/init.mp4".to_string()),
            media: Some("$RepresentationID$/$Time$.m4s".to_string()),
            SegmentTimeline: Some(SegmentTimeline { segments: timeline }),
            ..Default::default()
        }
    }

    fn s(t: Option<u64>, d: u64, r: Option<i64>) -> S {
        S {
            t,
            d,
            r,
            ..Default::default()
        }
    }

    fn rep(id: &str, template: SegmentTemplate) -> Representation {
        Representation {
            id: Some(id.to_string()),
            SegmentTemplate: Some(template),
            ..Default::default()
        }
    }

    fn set_with(reps: Vec<Representation>) -> AdaptationSet {
        AdaptationSet {
            representations: reps,
            ..Default::default()
        }
    }

    fn mpd_with(sets: Vec<AdaptationSet>) -> MPD {
        MPD {
            periods: vec![Period {
                adaptations: sets,
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn identical_templates_hoist_to_adaptation_set() {
        let t = tmpl(90000, vec![s(Some(0), 180000, Some(4))]);
        let mut set = set_with(vec![rep("v0", t.clone()), rep("v1", t.clone())]);
        hoist_shared_template(&mut set);
        assert_eq!(set.SegmentTemplate.as_ref(), Some(&t));
        assert!(set
            .representations
            .iter()
            .all(|r| r.SegmentTemplate.is_none()));
    }

    #[test]
    fn single_representation_set_hoists_template() {
        let t = tmpl(48000, vec![s(Some(0), 96000, Some(4))]);
        let mut set = set_with(vec![rep("a0", t.clone())]);
        hoist_shared_template(&mut set);
        assert_eq!(set.SegmentTemplate.as_ref(), Some(&t));
        assert!(set.representations[0].SegmentTemplate.is_none());
    }

    #[test]
    fn differing_templates_are_left_per_representation() {
        let a = tmpl(90000, vec![s(Some(0), 180000, Some(4))]);
        let b = tmpl(90000, vec![s(Some(0), 150000, Some(4))]);
        let mut set = set_with(vec![rep("v0", a.clone()), rep("v1", b.clone())]);
        hoist_shared_template(&mut set);
        assert!(set.SegmentTemplate.is_none());
        assert_eq!(set.representations[0].SegmentTemplate.as_ref(), Some(&a));
        assert_eq!(set.representations[1].SegmentTemplate.as_ref(), Some(&b));
    }

    #[test]
    fn compact_traverses_all_periods_and_sets() {
        let t = tmpl(90000, vec![s(Some(0), 180000, Some(4))]);
        let mut mpd = mpd_with(vec![set_with(vec![
            rep("v0", t.clone()),
            rep("v1", t.clone()),
        ])]);
        compact(&mut mpd);
        assert_eq!(
            mpd.periods[0].adaptations[0].SegmentTemplate.as_ref(),
            Some(&t)
        );
    }

    #[test]
    fn common_attributes_hoist_leaving_differing_timeline() {
        // Same timescale/media/init/PTO, different SegmentTimeline durations.
        let a = tmpl(90000, vec![s(Some(0), 180000, Some(4))]);
        let b = tmpl(90000, vec![s(Some(0), 150000, Some(4))]);
        let mut set = set_with(vec![rep("v0", a), rep("v1", b)]);
        hoist_shared_attributes(&mut set);

        let shared = set.SegmentTemplate.as_ref().unwrap();
        assert_eq!(shared.timescale, Some(90000));
        assert_eq!(shared.media.as_deref(), Some("$RepresentationID$/$Time$.m4s"));
        assert_eq!(
            shared.initialization.as_deref(),
            Some("$RepresentationID$/init.mp4")
        );
        assert_eq!(shared.presentationTimeOffset, Some(0));
        // The differing element stays per-rep; the common ones are cleared at set level.
        assert!(shared.SegmentTimeline.is_none());

        let r0 = set.representations[0].SegmentTemplate.as_ref().unwrap();
        assert_eq!(r0.timescale, None);
        assert_eq!(r0.media, None);
        assert_eq!(r0.SegmentTimeline.as_ref().unwrap().segments[0].d, 180000);
        let r1 = set.representations[1].SegmentTemplate.as_ref().unwrap();
        assert_eq!(r1.SegmentTimeline.as_ref().unwrap().segments[0].d, 150000);
    }

    #[test]
    fn attribute_hoist_empties_and_drops_identical_rep_templates() {
        // Called on identical reps, method 2 alone subsumes method 1: every field is
        // common, so reps empty out and are dropped to None.
        let t = tmpl(90000, vec![s(Some(0), 180000, Some(4))]);
        let mut set = set_with(vec![rep("v0", t.clone()), rep("v1", t.clone())]);
        hoist_shared_attributes(&mut set);
        assert_eq!(set.SegmentTemplate.as_ref(), Some(&t));
        assert!(set.representations.iter().all(|r| r.SegmentTemplate.is_none()));
    }

    #[test]
    fn compact_hoists_common_attributes_when_timelines_differ() {
        let a = tmpl(90000, vec![s(Some(0), 180000, Some(4))]);
        let b = tmpl(90000, vec![s(Some(0), 150000, Some(4))]);
        let mut mpd = mpd_with(vec![set_with(vec![rep("v0", a), rep("v1", b)])]);
        compact(&mut mpd);
        let set = &mpd.periods[0].adaptations[0];
        let shared = set.SegmentTemplate.as_ref().unwrap();
        assert_eq!(shared.media.as_deref(), Some("$RepresentationID$/$Time$.m4s"));
        assert!(shared.SegmentTimeline.is_none());
        assert!(set.representations[0]
            .SegmentTemplate
            .as_ref()
            .unwrap()
            .SegmentTimeline
            .is_some());
    }

    #[test]
    fn compact_handles_each_set_independently() {
        let v = tmpl(90000, vec![s(Some(0), 180000, Some(4))]);
        let a = tmpl(48000, vec![s(Some(0), 96000, Some(4))]);
        let mut mpd = mpd_with(vec![
            set_with(vec![rep("v0", v.clone()), rep("v1", v.clone())]),
            set_with(vec![rep("a0", a.clone())]),
        ]);
        compact(&mut mpd);
        assert_eq!(
            mpd.periods[0].adaptations[0].SegmentTemplate.as_ref(),
            Some(&v)
        );
        assert_eq!(
            mpd.periods[0].adaptations[1].SegmentTemplate.as_ref(),
            Some(&a)
        );
    }

    #[test]
    fn compact_is_idempotent() {
        let a = tmpl(90000, vec![s(Some(0), 180000, Some(4))]);
        let b = tmpl(90000, vec![s(Some(0), 150000, Some(4))]);
        let mut once = mpd_with(vec![set_with(vec![rep("v0", a), rep("v1", b)])]);
        compact(&mut once);
        let mut twice = once.clone();
        compact(&mut twice);
        assert_eq!(once, twice);
    }
}
