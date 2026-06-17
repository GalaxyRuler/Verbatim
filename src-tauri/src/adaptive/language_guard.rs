use crate::adaptive::language::analyze_language;
use crate::adaptive::types::LanguageClass;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExpectedScript {
    Latin,
    Arabic,
    Cyrillic,
    Cjk,
    Hangul,
    Hebrew,
    Devanagari,
    Greek,
    Thai,
    Armenian,
    Georgian,
}

pub fn expected_script(selected_language: &str) -> Option<ExpectedScript> {
    let normalized = selected_language.to_ascii_lowercase();
    let base = normalized
        .split(['-', '_'])
        .next()
        .unwrap_or(normalized.as_str());

    match base {
        "ar" | "fa" | "ur" | "ps" | "sd" => Some(ExpectedScript::Arabic),
        "ru" | "uk" | "bg" | "sr" | "mk" | "be" | "kk" | "ky" | "mn" => {
            Some(ExpectedScript::Cyrillic)
        }
        "ja" | "zh" => Some(ExpectedScript::Cjk),
        "ko" => Some(ExpectedScript::Hangul),
        "he" | "iw" | "yi" => Some(ExpectedScript::Hebrew),
        "hi" | "mr" | "ne" | "sa" => Some(ExpectedScript::Devanagari),
        "el" => Some(ExpectedScript::Greek),
        "th" => Some(ExpectedScript::Thai),
        "hy" => Some(ExpectedScript::Armenian),
        "ka" => Some(ExpectedScript::Georgian),
        "en" | "fr" | "de" | "es" | "it" | "pt" | "nl" | "sv" | "pl" | "da" | "nb" | "fi"
        | "tr" | "id" | "ms" | "vi" | "ca" | "ro" | "cs" | "sk" | "sl" | "hr" | "hu" | "et"
        | "lv" | "lt" | "sq" | "af" => Some(ExpectedScript::Latin),
        _ => None,
    }
}

pub fn contradicts_locked_language(selected_language: &str, text: &str) -> bool {
    let Some(expected) = expected_script(selected_language) else {
        return false;
    };

    let analysis = analyze_language(text, &[]);
    if matches!(
        analysis.class,
        LanguageClass::Empty | LanguageClass::Mixed | LanguageClass::TechnicalMixed
    ) {
        return false;
    }

    let Some(actual) = dominant_script(text) else {
        return false;
    };

    actual != expected
}

fn dominant_script(text: &str) -> Option<ExpectedScript> {
    let mut counts = ScriptCounts::default();
    let mut total = 0usize;

    for ch in text.chars().filter(|ch| ch.is_alphabetic()) {
        let Some(script) = script_for_char(ch) else {
            continue;
        };
        counts.increment(script);
        total += 1;
    }

    if total < 3 {
        return None;
    }

    let (script, count) = counts.max()?;
    if count * 100 >= total * 70 {
        Some(script)
    } else {
        None
    }
}

#[derive(Default)]
struct ScriptCounts {
    latin: usize,
    arabic: usize,
    cyrillic: usize,
    cjk: usize,
    hangul: usize,
    hebrew: usize,
    devanagari: usize,
    greek: usize,
    thai: usize,
    armenian: usize,
    georgian: usize,
}

impl ScriptCounts {
    fn increment(&mut self, script: ExpectedScript) {
        match script {
            ExpectedScript::Latin => self.latin += 1,
            ExpectedScript::Arabic => self.arabic += 1,
            ExpectedScript::Cyrillic => self.cyrillic += 1,
            ExpectedScript::Cjk => self.cjk += 1,
            ExpectedScript::Hangul => self.hangul += 1,
            ExpectedScript::Hebrew => self.hebrew += 1,
            ExpectedScript::Devanagari => self.devanagari += 1,
            ExpectedScript::Greek => self.greek += 1,
            ExpectedScript::Thai => self.thai += 1,
            ExpectedScript::Armenian => self.armenian += 1,
            ExpectedScript::Georgian => self.georgian += 1,
        }
    }

    fn max(&self) -> Option<(ExpectedScript, usize)> {
        [
            (ExpectedScript::Latin, self.latin),
            (ExpectedScript::Arabic, self.arabic),
            (ExpectedScript::Cyrillic, self.cyrillic),
            (ExpectedScript::Cjk, self.cjk),
            (ExpectedScript::Hangul, self.hangul),
            (ExpectedScript::Hebrew, self.hebrew),
            (ExpectedScript::Devanagari, self.devanagari),
            (ExpectedScript::Greek, self.greek),
            (ExpectedScript::Thai, self.thai),
            (ExpectedScript::Armenian, self.armenian),
            (ExpectedScript::Georgian, self.georgian),
        ]
        .into_iter()
        .filter(|(_, count)| *count > 0)
        .max_by_key(|(_, count)| *count)
    }
}

fn script_for_char(ch: char) -> Option<ExpectedScript> {
    let code = ch as u32;
    match code {
        0x0041..=0x005A
        | 0x0061..=0x007A
        | 0x00C0..=0x024F
        | 0x1E00..=0x1EFF
        | 0x2C60..=0x2C7F
        | 0xA720..=0xA7FF => Some(ExpectedScript::Latin),
        0x0600..=0x06FF | 0x0750..=0x077F | 0x08A0..=0x08FF | 0xFB50..=0xFDFF | 0xFE70..=0xFEFF => {
            Some(ExpectedScript::Arabic)
        }
        0x0400..=0x052F | 0x2DE0..=0x2DFF | 0xA640..=0xA69F => Some(ExpectedScript::Cyrillic),
        0x3040..=0x30FF | 0x31F0..=0x31FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF => {
            Some(ExpectedScript::Cjk)
        }
        0x1100..=0x11FF | 0x3130..=0x318F | 0xAC00..=0xD7AF => Some(ExpectedScript::Hangul),
        0x0590..=0x05FF => Some(ExpectedScript::Hebrew),
        0x0900..=0x097F | 0xA8E0..=0xA8FF => Some(ExpectedScript::Devanagari),
        0x0370..=0x03FF | 0x1F00..=0x1FFF => Some(ExpectedScript::Greek),
        0x0E00..=0x0E7F => Some(ExpectedScript::Thai),
        0x0530..=0x058F | 0xFB13..=0xFB17 => Some(ExpectedScript::Armenian),
        0x10A0..=0x10FF | 0x1C90..=0x1CBF => Some(ExpectedScript::Georgian),
        _ => None,
    }
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
    fn russian_lock_flags_latin_output() {
        assert!(contradicts_locked_language(
            "ru",
            "This is a clear English sentence"
        ));
    }

    #[test]
    fn japanese_lock_flags_latin_output() {
        assert!(contradicts_locked_language(
            "ja",
            "This is a clear English sentence"
        ));
    }

    #[test]
    fn hebrew_lock_flags_latin_output() {
        assert!(contradicts_locked_language(
            "he",
            "This is a clear English sentence"
        ));
    }

    #[test]
    fn korean_lock_flags_latin_output() {
        assert!(contradicts_locked_language(
            "ko",
            "This is a clear English sentence"
        ));
    }

    #[test]
    fn latin_lock_flags_cyrillic_output() {
        assert!(contradicts_locked_language("en", "Это русский текст"));
    }

    #[test]
    fn latin_lock_flags_cjk_output() {
        assert!(contradicts_locked_language("en", "これは日本語の文章です"));
    }

    #[test]
    fn latin_lock_allows_english_output() {
        assert!(!contradicts_locked_language(
            "en",
            "This is a clear English sentence"
        ));
    }

    #[test]
    fn russian_lock_allows_cyrillic_output() {
        assert!(!contradicts_locked_language("ru", "Это русский текст"));
    }

    #[test]
    fn japanese_lock_allows_cjk_output() {
        assert!(!contradicts_locked_language("ja", "これは日本語の文章です"));
    }

    #[test]
    fn hebrew_lock_allows_hebrew_output() {
        assert!(!contradicts_locked_language("he", "זה טקסט בעברית"));
    }

    #[test]
    fn korean_lock_allows_hangul_output() {
        assert!(!contradicts_locked_language(
            "ko",
            "이것은 한국어 문장입니다"
        ));
    }

    #[test]
    fn auto_never_blocks() {
        assert!(!contradicts_locked_language("auto", "هذا نص عربي واضح"));
    }

    #[test]
    fn unmapped_language_never_blocks() {
        assert!(!contradicts_locked_language(
            "unknown",
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
        assert_eq!(expected_script("ru"), Some(ExpectedScript::Cyrillic));
        assert_eq!(expected_script("ja"), Some(ExpectedScript::Cjk));
        assert_eq!(expected_script("zh"), Some(ExpectedScript::Cjk));
        assert_eq!(expected_script("zh-Hans"), Some(ExpectedScript::Cjk));
        assert_eq!(expected_script("ko"), Some(ExpectedScript::Hangul));
        assert_eq!(expected_script("he"), Some(ExpectedScript::Hebrew));

        for code in [
            "en", "fr", "de", "es", "it", "pt", "nl", "sv", "pl", "da", "nb", "fi", "tr", "id",
            "ms", "vi", "ca", "ro",
        ] {
            assert_eq!(expected_script(code), Some(ExpectedScript::Latin));
        }

        assert_eq!(expected_script("auto"), None);
        assert_eq!(expected_script("unknown"), None);
    }
}
