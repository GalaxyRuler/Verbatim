use crate::adaptive::types::{LanguageAnalysis, LanguageClass};

pub fn analyze_language(text: &str, shortlist: &[String]) -> LanguageAnalysis {
    let mut arabic = 0usize;
    let mut latin = 0usize;
    let mut letters = 0usize;

    for ch in text.chars() {
        if ch.is_alphabetic() {
            letters += 1;
            if is_arabic_char(ch) {
                arabic += 1;
            } else if ch.is_ascii_alphabetic() {
                latin += 1;
            }
        }
    }

    let technical_token_count = text
        .split_whitespace()
        .filter(|token| is_technical_token(token))
        .count();
    let contains_url = text.contains("://") || text.starts_with("www.") || text.contains(" www.");
    let contains_identifier = text.split_whitespace().any(|token| {
        token.contains('_') || token.contains("::") || token.contains('/') || token.contains('\\')
    });

    let arabic_ratio = ratio(arabic, letters);
    let latin_ratio = ratio(latin, letters);

    let class = if text.trim().is_empty() {
        LanguageClass::Empty
    } else if technical_token_count >= 2 || contains_url || contains_identifier {
        LanguageClass::TechnicalMixed
    } else if arabic_ratio >= 0.20 && latin_ratio >= 0.20 {
        LanguageClass::Mixed
    } else if arabic_ratio >= 0.70 {
        LanguageClass::MostlyArabic
    } else if latin_ratio >= 0.70 {
        LanguageClass::MostlyLatin
    } else {
        LanguageClass::Unknown
    };

    LanguageAnalysis {
        class,
        shortlist: shortlist.to_vec(),
        arabic_ratio,
        latin_ratio,
        technical_token_count,
        contains_url,
        contains_identifier,
    }
}

fn ratio(count: usize, total: usize) -> f32 {
    if total == 0 {
        0.0
    } else {
        count as f32 / total as f32
    }
}

fn is_arabic_char(ch: char) -> bool {
    matches!(
        ch as u32,
        0x0600..=0x06FF | 0x0750..=0x077F | 0x08A0..=0x08FF | 0xFB50..=0xFDFF | 0xFE70..=0xFEFF
    )
}

pub(crate) fn is_technical_token(token: &str) -> bool {
    let lower = token
        .trim_matches(|ch: char| ch.is_ascii_punctuation())
        .to_lowercase();
    lower.contains('.')
        || lower.contains('/')
        || lower.contains('\\')
        || lower.contains("::")
        || lower.contains('_')
        || matches!(
            lower.as_str(),
            "cargo"
                | "git"
                | "npm"
                | "bun"
                | "pnpm"
                | "run"
                | "test"
                | "build"
                | "src"
                | "api"
                | "json"
                | "yaml"
                | "toml"
                | "rs"
                | "tsx"
                | "ts"
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_mostly_arabic() {
        let result = analyze_language("هذا نص عربي واضح", &["en".to_string(), "ar".to_string()]);
        assert_eq!(result.class, LanguageClass::MostlyArabic);
        assert!(result.arabic_ratio > 0.70);
    }

    #[test]
    fn classifies_mostly_latin() {
        let result = analyze_language(
            "This is a clear English sentence",
            &["en".to_string(), "ar".to_string()],
        );
        assert_eq!(result.class, LanguageClass::MostlyLatin);
        assert!(result.latin_ratio > 0.70);
    }

    #[test]
    fn classifies_mixed_arabic_english() {
        let result = analyze_language(
            "خلينا update the config بعدين",
            &["en".to_string(), "ar".to_string()],
        );
        assert_eq!(result.class, LanguageClass::Mixed);
        assert!(result.arabic_ratio > 0.20);
        assert!(result.latin_ratio > 0.20);
    }

    #[test]
    fn classifies_technical_mixed() {
        let result = analyze_language(
            "run cargo test and check src-tauri/src/actions.rs",
            &["en".to_string(), "ar".to_string()],
        );
        assert_eq!(result.class, LanguageClass::TechnicalMixed);
        assert!(result.technical_token_count >= 3);
        assert!(result.contains_identifier);
    }

    #[test]
    fn detects_urls_without_treating_them_as_translation_signal() {
        let result = analyze_language(
            "send it to https://example.com/api/v1",
            &["en".to_string(), "fr".to_string()],
        );
        assert!(result.contains_url);
        assert_eq!(result.class, LanguageClass::TechnicalMixed);
    }
}
