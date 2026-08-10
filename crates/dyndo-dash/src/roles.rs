use dash_mpd::{Accessibility, Role as DashRole};
use dyndo_core::role::Role;

const ROLE_SCHEME: &str = "urn:mpeg:dash:role:2011";

pub(super) fn roles(content_type: &str, role: Option<Role>) -> Vec<DashRole> {
    let value = match (content_type, role) {
        ("text", None | Some(Role::Caption)) => "subtitle",
        ("audio", Some(Role::Description | Role::EnhancedAudioIntelligibility)) | (_, None) => {
            return Vec::new();
        }
        (_, Some(role)) => role.as_str(),
    };

    vec![DashRole {
        schemeIdUri: ROLE_SCHEME.to_string(),
        value: Some(value.to_string()),
        ..Default::default()
    }]
}

pub(super) fn accessibility(content_type: &str, role: Option<Role>) -> Vec<Accessibility> {
    let value = match (content_type, role) {
        ("audio", Some(Role::Description)) => "description",
        ("audio", Some(Role::EnhancedAudioIntelligibility)) => "enhanced-audio-intelligibility",
        ("text", Some(Role::Caption)) => "caption",
        _ => return Vec::new(),
    };

    vec![Accessibility {
        schemeIdUri: ROLE_SCHEME.to_string(),
        value: Some(value.to_string()),
        id: None,
    }]
}
