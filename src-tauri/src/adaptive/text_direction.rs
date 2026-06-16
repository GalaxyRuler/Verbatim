use crate::adaptive::language::analyze_language;
use crate::adaptive::types::{LanguageClass, TargetKind};

const LEFT_TO_RIGHT_MARK: char = '\u{200E}';

pub fn stabilize_ltr_paste_text(text: &str, target_kind: &TargetKind) -> String {
    if !should_stabilize_ltr_paste_text(text, target_kind) {
        return text.to_string();
    }

    text.split('\n')
        .map(stabilize_ltr_line)
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn should_stabilize_ltr_paste_text(text: &str, target_kind: &TargetKind) -> bool {
    if target_kind == &TargetKind::Technical || text.trim().is_empty() {
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
        let result = stabilize_ltr_paste_text(
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
        assert_eq!(stabilize_ltr_paste_text(text, &TargetKind::Email), text);
    }

    #[test]
    fn wraps_ltr_notes_lines_with_ltr_marks() {
        let result =
            stabilize_ltr_paste_text("I like simple notes and I kind of lie.", &TargetKind::Notes);

        assert_eq!(
            result,
            "\u{200E}I like simple notes and I kind of lie.\u{200E}"
        );
    }

    #[test]
    fn wraps_unknown_ltr_prose_with_ltr_marks() {
        let result =
            stabilize_ltr_paste_text("This should stay left aligned.", &TargetKind::Unknown);

        assert_eq!(result, "\u{200E}This should stay left aligned.\u{200E}");
    }

    #[test]
    fn leaves_technical_ltr_text_unchanged() {
        let text = "Dear James,\n\nHow did you come to them?";
        assert_eq!(stabilize_ltr_paste_text(text, &TargetKind::Technical), text);
    }
}
