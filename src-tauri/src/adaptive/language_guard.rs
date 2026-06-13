use crate::adaptive::language::analyze_language;
use crate::adaptive::types::LanguageClass;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExpectedScript {
    Latin,
    Arabic,
}

pub fn expected_script(selected_language: &str) -> Option<ExpectedScript> {
    match selected_language {
        "ar" => Some(ExpectedScript::Arabic),
        "en" | "fr" | "de" | "es" | "it" | "pt" | "nl" | "sv" | "pl" | "da" | "nb" | "fi"
        | "tr" | "id" | "ms" | "vi" | "ca" | "ro" => Some(ExpectedScript::Latin),
        _ => None,
    }
}

pub fn contradicts_locked_language(selected_language: &str, text: &str) -> bool {
    let Some(expected) = expected_script(selected_language) else {
        return false;
    };

    let analysis = analyze_language(text, &[]);

    matches!(
        (expected, analysis.class),
        (ExpectedScript::Latin, LanguageClass::MostlyArabic)
            | (ExpectedScript::Arabic, LanguageClass::MostlyLatin)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latin_lock_flags_arabic_output() {
        assert!(contradicts_locked_language("en", "هذا نص عربي واضح"));
    }

    #[test]
    fn arabic_lock_flags_latin_output() {
        assert!(contradicts_locked_language(
            "ar",
            "This is a clear English sentence"
        ));
    }

    #[test]
    fn latin_lock_allows_english_output() {
        assert!(!contradicts_locked_language(
            "en",
            "This is a clear English sentence"
        ));
    }

    #[test]
    fn auto_never_blocks() {
        assert!(!contradicts_locked_language("auto", "هذا نص عربي واضح"));
    }

    #[test]
    fn unmapped_language_never_blocks() {
        assert!(!contradicts_locked_language(
            "zh-Hans",
            "This is a clear English sentence"
        ));
    }

    #[test]
    fn empty_output_is_not_blocked() {
        assert!(!contradicts_locked_language("en", ""));
    }

    #[test]
    fn unknown_output_is_not_blocked() {
        assert!(!contradicts_locked_language("en", "12345 !!!"));
    }

    #[test]
    fn mixed_output_is_not_blocked() {
        assert!(!contradicts_locked_language(
            "en",
            "خلينا update the config بعدين"
        ));
    }

    #[test]
    fn technical_output_is_not_blocked() {
        assert!(!contradicts_locked_language(
            "ar",
            "run cargo test and check src-tauri/src/actions.rs"
        ));
    }

    #[test]
    fn expected_script_maps_known_codes() {
        assert_eq!(expected_script("ar"), Some(ExpectedScript::Arabic));

        for code in [
            "en", "fr", "de", "es", "it", "pt", "nl", "sv", "pl", "da", "nb", "fi", "tr", "id",
            "ms", "vi", "ca", "ro",
        ] {
            assert_eq!(expected_script(code), Some(ExpectedScript::Latin));
        }

        assert_eq!(expected_script("auto"), None);
        assert_eq!(expected_script("zh-Hans"), None);
        assert_eq!(expected_script("unknown"), None);
    }
}
