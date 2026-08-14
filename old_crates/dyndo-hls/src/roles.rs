use dyndo_core::role::Role;
use isolang::Language;
use language_tags::LanguageTag;

const DESCRIBES_VIDEO: &str = "public.accessibility.describes-video";
const ENHANCES_SPEECH: &str = "public.accessibility.enhances-speech";
const CAPTIONS: &str = concat!(
    "public.accessibility.transcribes-spoken-dialog,",
    "public.accessibility.describes-music-and-sound"
);

pub(crate) fn name(language: &LanguageTag, role: Option<Role>) -> String {
    let name = language_name(language);
    role.map_or_else(
        || name.to_string(),
        |role| format!("{name} ({})", label(role)),
    )
}

fn language_name(language: &LanguageTag) -> &str {
    let primary = language.primary_language();
    let parsed = match primary.len() {
        2 => Language::from_639_1(primary),
        3 => Language::from_639_3(primary),
        _ => None,
    };
    parsed.map_or(language.as_str(), |language| language.to_name())
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
