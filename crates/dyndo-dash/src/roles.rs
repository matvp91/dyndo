use dash_mpd::{Accessibility, Role as DashRole};
use dyndo_core::role::Role;

const ROLE_SCHEME: &str = "urn:mpeg:dash:role:2011";

pub(super) fn role(value: Option<Role>, text: bool, audio: bool) -> Vec<DashRole> {
    let value = match (value, text, audio) {
        (None, true, _) | (Some(Role::Caption), true, _) => "subtitle".to_owned(),
        (Some(Role::Description | Role::EnhancedAudioIntelligibility), _, true) => {
            return Vec::new();
        }
        (Some(value), _, _) => value.to_string(),
        (None, _, _) => return Vec::new(),
    };
    vec![DashRole {
        schemeIdUri: ROLE_SCHEME.into(),
        value: Some(value),
        ..Default::default()
    }]
}

pub(super) fn accessibility(value: Option<Role>, text: bool, audio: bool) -> Vec<Accessibility> {
    let value = match (value, text, audio) {
        (Some(Role::Caption), true, _) => "caption",
        (Some(Role::Description), _, true) => "description",
        (Some(Role::EnhancedAudioIntelligibility), _, true) => "enhanced-audio-intelligibility",
        _ => return Vec::new(),
    };
    vec![Accessibility {
        schemeIdUri: ROLE_SCHEME.into(),
        value: Some(value.into()),
        id: None,
    }]
}
