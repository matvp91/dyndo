use dash_mpd::{Accessibility, Role as DashRole};
use dyndo_core::role::Role;
use dyndo_core::track::TrackType;

const ROLE_SCHEME: &str = "urn:mpeg:dash:role:2011";

pub(super) fn roles(track_type: TrackType, role: Option<Role>) -> Vec<DashRole> {
    let value = match (track_type, role) {
        (TrackType::Text, None | Some(Role::Caption)) => "subtitle",
        (TrackType::Audio, Some(Role::Description | Role::EnhancedAudioIntelligibility))
        | (_, None) => {
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

pub(super) fn accessibility(track_type: TrackType, role: Option<Role>) -> Vec<Accessibility> {
    let value = match (track_type, role) {
        (TrackType::Audio, Some(Role::Description)) => "description",
        (TrackType::Audio, Some(Role::EnhancedAudioIntelligibility)) => {
            "enhanced-audio-intelligibility"
        }
        (TrackType::Text, Some(Role::Caption)) => "caption",
        _ => return Vec::new(),
    };

    vec![Accessibility {
        schemeIdUri: ROLE_SCHEME.to_string(),
        value: Some(value.to_string()),
        id: None,
    }]
}
