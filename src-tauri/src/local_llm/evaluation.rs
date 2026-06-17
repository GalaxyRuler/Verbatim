#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalLlmEvaluationIssue {
    EmptyOutput,
    ExcessiveExpansion,
    LostLatinScript,
    LostArabicScript,
    LostHebrewScript,
    LostCyrillicScript,
    LostCjkScript,
    LostSourceTerms,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalLlmOutputEvaluation {
    pub passed: bool,
    pub issues: Vec<LocalLlmEvaluationIssue>,
}

impl LocalLlmOutputEvaluation {
    pub fn has_issue(&self, issue: LocalLlmEvaluationIssue) -> bool {
        self.issues.contains(&issue)
    }
}

#[derive(Debug, Clone, Copy)]
enum ScriptKind {
    Latin,
    Arabic,
    Hebrew,
    Cyrillic,
    Cjk,
}

pub fn evaluate_post_processing_output(input: &str, output: &str) -> LocalLlmOutputEvaluation {
    let mut issues = Vec::new();
    let input = input.trim();
    let output = output.trim();

    if output.is_empty() {
        issues.push(LocalLlmEvaluationIssue::EmptyOutput);
    }

    if is_excessively_expanded(input, output) {
        issues.push(LocalLlmEvaluationIssue::ExcessiveExpansion);
    }

    for script in [
        ScriptKind::Latin,
        ScriptKind::Arabic,
        ScriptKind::Hebrew,
        ScriptKind::Cyrillic,
        ScriptKind::Cjk,
    ] {
        if contains_script(input, script) && !contains_script(output, script) {
            issues.push(lost_script_issue(script));
        }
    }

    if loses_required_source_terms(input, output) {
        issues.push(LocalLlmEvaluationIssue::LostSourceTerms);
    }

    LocalLlmOutputEvaluation {
        passed: issues.is_empty(),
        issues,
    }
}

fn is_excessively_expanded(input: &str, output: &str) -> bool {
    let input_chars = input.chars().count();
    let output_chars = output.chars().count();

    input_chars > 0 && output_chars > input_chars.saturating_mul(3).max(input_chars + 200)
}

fn lost_script_issue(script: ScriptKind) -> LocalLlmEvaluationIssue {
    match script {
        ScriptKind::Latin => LocalLlmEvaluationIssue::LostLatinScript,
        ScriptKind::Arabic => LocalLlmEvaluationIssue::LostArabicScript,
        ScriptKind::Hebrew => LocalLlmEvaluationIssue::LostHebrewScript,
        ScriptKind::Cyrillic => LocalLlmEvaluationIssue::LostCyrillicScript,
        ScriptKind::Cjk => LocalLlmEvaluationIssue::LostCjkScript,
    }
}

fn contains_script(text: &str, script: ScriptKind) -> bool {
    text.chars().any(|ch| match script {
        ScriptKind::Latin => ch.is_ascii_alphabetic() || matches!(ch, '\u{00C0}'..='\u{024F}'),
        ScriptKind::Arabic => matches!(
            ch,
            '\u{0600}'..='\u{06FF}'
                | '\u{0750}'..='\u{077F}'
                | '\u{08A0}'..='\u{08FF}'
                | '\u{FB50}'..='\u{FDFF}'
                | '\u{FE70}'..='\u{FEFF}'
        ),
        ScriptKind::Hebrew => matches!(ch, '\u{0590}'..='\u{05FF}'),
        ScriptKind::Cyrillic => matches!(
            ch,
            '\u{0400}'..='\u{04FF}' | '\u{0500}'..='\u{052F}' | '\u{2DE0}'..='\u{2DFF}'
        ),
        ScriptKind::Cjk => matches!(
            ch,
            '\u{3040}'..='\u{30FF}'
                | '\u{3400}'..='\u{4DBF}'
                | '\u{4E00}'..='\u{9FFF}'
                | '\u{AC00}'..='\u{D7AF}'
        ),
    })
}

fn loses_required_source_terms(input: &str, output: &str) -> bool {
    let input_terms = required_source_terms(input);
    if input_terms.is_empty() || input_terms.len() > 8 {
        return false;
    }

    let output_terms = normalized_terms(output);
    input_terms.iter().any(|term| !output_terms.contains(term))
}

fn required_source_terms(text: &str) -> std::collections::HashSet<String> {
    normalized_terms(text)
        .into_iter()
        .filter(|term| term.chars().count() >= 3 && !ignored_source_term(term))
        .collect()
}

fn normalized_terms(text: &str) -> std::collections::HashSet<String> {
    let mut terms = std::collections::HashSet::new();
    let mut current = String::new();

    for ch in text.chars() {
        if ch.is_alphanumeric() {
            current.extend(ch.to_lowercase());
        } else if !current.is_empty() {
            terms.insert(normalize_term(&current));
            current.clear();
        }
    }

    if !current.is_empty() {
        terms.insert(normalize_term(&current));
    }

    terms
}

fn normalize_term(term: &str) -> String {
    term.chars()
        .filter(|ch| !is_combining_mark(*ch))
        .map(|ch| match ch {
            'إ' | 'أ' | 'آ' | 'ٱ' => 'ا',
            'ى' => 'ي',
            'ة' => 'ه',
            other => other,
        })
        .collect()
}

fn is_combining_mark(ch: char) -> bool {
    matches!(
        ch,
        '\u{0300}'..='\u{036F}'
            | '\u{0610}'..='\u{061A}'
            | '\u{064B}'..='\u{065F}'
            | '\u{0670}'
            | '\u{06D6}'..='\u{06DC}'
            | '\u{06DF}'..='\u{06E4}'
            | '\u{06E7}'..='\u{06E8}'
            | '\u{06EA}'..='\u{06ED}'
    )
}

fn ignored_source_term(term: &str) -> bool {
    matches!(
        term,
        "and"
            | "are"
            | "but"
            | "comma"
            | "dash"
            | "dot"
            | "mark"
            | "new"
            | "paragraph"
            | "period"
            | "question"
            | "the"
            | "to"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_output() {
        let evaluation = evaluate_post_processing_output("hello there", "");

        assert!(!evaluation.passed);
        assert!(evaluation.has_issue(LocalLlmEvaluationIssue::EmptyOutput));
    }

    #[test]
    fn rejects_excessive_expansion() {
        let expanded = "hello ".repeat(80);

        let evaluation = evaluate_post_processing_output("hello", &expanded);

        assert!(!evaluation.passed);
        assert!(evaluation.has_issue(LocalLlmEvaluationIssue::ExcessiveExpansion));
    }

    #[test]
    fn rejects_lost_non_arabic_scripts_too() {
        let evaluation = evaluate_post_processing_output(
            "please keep пример as written",
            "please keep primer as written",
        );

        assert!(!evaluation.passed);
        assert!(evaluation.has_issue(LocalLlmEvaluationIssue::LostCyrillicScript));
    }

    #[test]
    fn accepts_light_cleanup_when_scripts_are_preserved() {
        let evaluation = evaluate_post_processing_output("hello comma خالد", "Hello, خالد.");

        assert!(evaluation.passed);
    }

    #[test]
    fn rejects_short_output_that_drops_source_terms() {
        let evaluation = evaluate_post_processing_output("email signature", "Regards,\nAbdullah");

        assert!(!evaluation.passed);
        assert!(evaluation.has_issue(LocalLlmEvaluationIssue::LostSourceTerms));
    }
}
