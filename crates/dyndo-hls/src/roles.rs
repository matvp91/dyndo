use dyndo_core::role::Role;

const DESCRIBES_VIDEO: &str = "public.accessibility.describes-video";
const ENHANCES_SPEECH: &str = "public.accessibility.enhances-speech";
const CAPTIONS: &str = concat!(
    "public.accessibility.transcribes-spoken-dialog,",
    "public.accessibility.describes-music-and-sound"
);

pub(crate) fn name(language: &str, role: Option<Role>) -> String {
    role.map_or_else(
        || language.to_string(),
        |role| format!("{language} ({})", label(role)),
    )
}

pub(crate) const fn characteristics(role: Option<Role>) -> Option<&'static str> {
    match role {
        Some(Role::Description) => Some(DESCRIBES_VIDEO),
        Some(Role::EnhancedAudioIntelligibility) => Some(ENHANCES_SPEECH),
        Some(Role::Caption) => Some(CAPTIONS),
        _ => None,
    }
}

pub(crate) const fn is_forced(role: Option<Role>) -> bool {
    matches!(role, Some(Role::ForcedSubtitle))
}

const fn label(role: Role) -> &'static str {
    match role {
        Role::Main => "Main",
        Role::Alternate => "Alternate",
        Role::Commentary => "Commentary",
        Role::Dub => "Dub",
        Role::Description => "Audio Description",
        Role::EnhancedAudioIntelligibility => "Enhanced Dialogue",
        Role::Subtitle => "Subtitles",
        Role::Caption => "Captions",
        Role::ForcedSubtitle => "Forced Subtitles",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_includes_the_human_readable_role() {
        assert_eq!(
            name("en", Some(Role::Description)),
            "en (Audio Description)"
        );
    }

    #[test]
    fn name_is_the_language_when_role_is_absent() {
        assert_eq!(name("en", None), "en");
    }

    #[test]
    fn caption_role_has_both_accessibility_characteristics() {
        assert_eq!(characteristics(Some(Role::Caption)), Some(CAPTIONS));
    }
}
