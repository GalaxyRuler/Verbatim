use crate::adaptive::language::analyze_language;
use crate::adaptive::types::{LanguageClass, TargetKind};

const LEFT_TO_RIGHT_MARK: char = '\u{200E}';

pub fn stabilize_ltr_email_paste_text(text: &str, target_kind: &TargetKind) -> String {
    if !should_stabilize_ltr_email_text(text, target_kind) {
        return text.to_string();
    }

    text.split('\n')
        .map(stabilize_ltr_line)
        .collect::<Vec<_>>()
        .join("\n")
}

fn should_stabilize_ltr_email_text(text: &str, target_kind: &TargetKind) -> bool {
    if target_kind != &TargetKind::Email || text.trim().is_empty() {
        return false;
    }

    let language = analyze_language(text, &[]);
    matches!(language.class, LanguageClass::MostlyLatin) && language.arabic_ratio < 0.10
}

fn stabilize_ltr_line(line: &str) -> String {
    if line.trim().is_empty() {
        return line.to_string();
    }

    let mut output = String::new();
    if !line.starts_with(LEFT_TO_RIGHT_MARK) {
        output.push(LEFT_TO_RIGHT_MARK);
    }
    output.push_str(line);
    if !line.ends_with(LEFT_TO_RIGHT_MARK) {
        output.push(LEFT_TO_RIGHT_MARK);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_ltr_email_lines_with_ltr_marks() {
        let result = stabilize_ltr_email_paste_text(
            "Dear James,\n\nHow did you come to them?\n\nSincerely,\nAbdullah",
            &TargetKind::Email,
        );

        assert_eq!(
            result,
            "\u{200E}Dear James,\u{200E}\n\n\u{200E}How did you come to them?\u{200E}\n\n\u{200E}Sincerely,\u{200E}\n\u{200E}Abdullah\u{200E}"
        );
    }

    #[test]
    fn leaves_arabic_email_text_unchanged() {
        let text = "عزيزي خالد،\n\nهل راجعت الملف؟";
        assert_eq!(
            stabilize_ltr_email_paste_text(text, &TargetKind::Email),
            text
        );
    }

    #[test]
    fn leaves_non_email_ltr_text_unchanged() {
        let text = "Dear James,\n\nHow did you come to them?";
        assert_eq!(
            stabilize_ltr_email_paste_text(text, &TargetKind::Technical),
            text
        );
    }
}
