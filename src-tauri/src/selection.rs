#[cfg(test)]
use crate::post_paste_learning::FocusedTextSnapshot;
use crate::post_paste_learning::{FocusedTextSelection, FocusedTextSelectionSnapshot};
use std::fmt;
use tauri::AppHandle;

#[derive(Clone, PartialEq, Eq)]
pub struct SelectionSnapshot {
    pub target_id: String,
    pub focused_text: String,
    pub selected_text: String,
}

#[derive(Clone, PartialEq, Eq)]
pub struct SelectionReplacementPlan {
    pub target_id: String,
    pub original_selected_text: String,
    pub replacement_text: String,
}

#[derive(Clone, PartialEq, Eq)]
pub struct SelectionReplacementRecovery {
    pub copy_text: String,
    pub original_selected_text: String,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelectionReplacementFailure {
    PasteFailed(String),
}

#[derive(Clone, PartialEq, Eq)]
pub enum SelectionReplacementOutcome {
    Replaced(SelectionReplacementPlan),
    Recoverable(SelectionReplacementRecovery),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelectionReplaceError {
    NoSelection,
    TargetChanged {
        expected_target_id: String,
        actual_target_id: String,
    },
    SelectionChanged {
        expected_len: usize,
        actual_len: usize,
    },
    FocusedTextChanged {
        expected_len: usize,
        actual_len: usize,
    },
    AmbiguousSelection {
        occurrence_count: usize,
    },
    CurrentSelectionUnavailable(SelectionCaptureError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelectionCaptureError {
    NoSelection,
    SecureField,
    SecureCheckError,
    Unavailable(String),
}

pub const SECURE_FIELD_REASON_CODE: &str = "secure_field";
pub const SECURE_CHECK_ERROR_REASON_CODE: &str = "secure_check_error";

impl SelectionCaptureError {
    pub const fn reason_code(&self) -> &'static str {
        match self {
            Self::NoSelection => "no_selection",
            Self::SecureField => SECURE_FIELD_REASON_CODE,
            Self::SecureCheckError => SECURE_CHECK_ERROR_REASON_CODE,
            Self::Unavailable(_) => "selection_unavailable",
        }
    }
}

impl fmt::Display for SelectionCaptureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason_code())
    }
}

impl std::error::Error for SelectionCaptureError {}

impl SelectionSnapshot {
    #[cfg(test)]
    pub fn from_focused_snapshot(
        focused: FocusedTextSnapshot,
        selected_text: impl Into<String>,
    ) -> Self {
        Self {
            target_id: focused.target_id,
            focused_text: focused.text,
            selected_text: selected_text.into(),
        }
    }
}

impl fmt::Debug for SelectionSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SelectionSnapshot")
            .field("target_id", &self.target_id)
            .field("focused_text_len", &self.focused_text.chars().count())
            .field("selected_text_len", &self.selected_text.chars().count())
            .finish()
    }
}

impl fmt::Debug for SelectionReplacementPlan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SelectionReplacementPlan")
            .field("target_id", &self.target_id)
            .field(
                "original_selected_text_len",
                &self.original_selected_text.chars().count(),
            )
            .field(
                "replacement_text_len",
                &self.replacement_text.chars().count(),
            )
            .finish()
    }
}

impl fmt::Debug for SelectionReplacementRecovery {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SelectionReplacementRecovery")
            .field("copy_text_len", &self.copy_text.chars().count())
            .field(
                "original_selected_text_len",
                &self.original_selected_text.chars().count(),
            )
            .field("reason", &self.reason)
            .finish()
    }
}

impl fmt::Debug for SelectionReplacementOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Replaced(plan) => formatter.debug_tuple("Replaced").field(plan).finish(),
            Self::Recoverable(recovery) => formatter
                .debug_tuple("Recoverable")
                .field(recovery)
                .finish(),
        }
    }
}

pub fn selection_snapshot_from_focused_text_selection(
    snapshot: FocusedTextSelectionSnapshot,
) -> Result<SelectionSnapshot, SelectionCaptureError> {
    let selected_text = match snapshot.selection {
        FocusedTextSelection::Selected(selected_text) if !selected_text.trim().is_empty() => {
            selected_text
        }
        FocusedTextSelection::Selected(_) | FocusedTextSelection::Empty => {
            return Err(SelectionCaptureError::NoSelection);
        }
        FocusedTextSelection::Unsupported(reason) => {
            return Err(SelectionCaptureError::Unavailable(reason));
        }
    };

    Ok(SelectionSnapshot {
        target_id: snapshot.target_id,
        focused_text: snapshot.text,
        selected_text,
    })
}

#[allow(dead_code)]
pub fn capture_current_selection_snapshot() -> Result<SelectionSnapshot, SelectionCaptureError> {
    capture_current_selection_snapshot_with(
        crate::post_paste_learning::capture_focused_text_selection_snapshot,
    )
}

fn capture_current_selection_snapshot_with<C>(
    capture: C,
) -> Result<SelectionSnapshot, SelectionCaptureError>
where
    C: FnOnce() -> Result<FocusedTextSelectionSnapshot, SelectionCaptureError>,
{
    let snapshot = capture()?;
    selection_snapshot_from_focused_text_selection(snapshot)
}

pub fn plan_selected_text_replacement(
    captured: &SelectionSnapshot,
    current: &SelectionSnapshot,
    replacement_text: &str,
) -> Result<SelectionReplacementPlan, SelectionReplaceError> {
    validate_selected_text_anchor(captured)?;

    if captured.target_id != current.target_id {
        return Err(SelectionReplaceError::TargetChanged {
            expected_target_id: captured.target_id.clone(),
            actual_target_id: current.target_id.clone(),
        });
    }

    if captured.focused_text != current.focused_text {
        return Err(SelectionReplaceError::FocusedTextChanged {
            expected_len: captured.focused_text.chars().count(),
            actual_len: current.focused_text.chars().count(),
        });
    }

    if captured.selected_text != current.selected_text {
        return Err(SelectionReplaceError::SelectionChanged {
            expected_len: captured.selected_text.chars().count(),
            actual_len: current.selected_text.chars().count(),
        });
    }

    Ok(SelectionReplacementPlan {
        target_id: captured.target_id.clone(),
        original_selected_text: captured.selected_text.clone(),
        replacement_text: replacement_text.to_string(),
    })
}

pub fn validate_selected_text_anchor(
    captured: &SelectionSnapshot,
) -> Result<(), SelectionReplaceError> {
    if captured.selected_text.trim().is_empty() {
        return Err(SelectionReplaceError::NoSelection);
    }

    let selected_text_occurrences =
        selected_text_occurrence_count(&captured.focused_text, &captured.selected_text);
    if selected_text_occurrences != 1 {
        return Err(SelectionReplaceError::AmbiguousSelection {
            occurrence_count: selected_text_occurrences,
        });
    }

    Ok(())
}

fn selected_text_occurrence_count(focused_text: &str, selected_text: &str) -> usize {
    if selected_text.is_empty() {
        return 0;
    }

    focused_text
        .char_indices()
        .filter(|(index, _)| focused_text[*index..].starts_with(selected_text))
        .count()
}

#[allow(dead_code)]
pub fn replace_current_selection_with(
    app: &AppHandle,
    replacement_text: &str,
) -> Result<SelectionReplacementOutcome, SelectionReplaceError> {
    let captured = capture_current_selection_snapshot()
        .map_err(SelectionReplaceError::CurrentSelectionUnavailable)?;
    replace_captured_selection(app, &captured, replacement_text)
}

#[allow(dead_code)]
pub fn replace_captured_selection(
    app: &AppHandle,
    captured: &SelectionSnapshot,
    replacement_text: &str,
) -> Result<SelectionReplacementOutcome, SelectionReplaceError> {
    replace_captured_selection_with_cancellation(app, captured, replacement_text, None)
}

pub fn replace_captured_selection_with_cancellation(
    app: &AppHandle,
    captured: &SelectionSnapshot,
    replacement_text: &str,
    is_cancelled: crate::clipboard::CancellationCheck<'_>,
) -> Result<SelectionReplacementOutcome, SelectionReplaceError> {
    replace_selected_text_after_recapture(
        captured,
        replacement_text,
        capture_current_selection_snapshot,
        |plan| {
            crate::clipboard::paste_exact_preserving_clipboard_with_cancellation(
                &plan.replacement_text,
                app,
                is_cancelled,
            )
        },
    )
}

fn replace_selected_text_after_recapture<C, R>(
    captured: &SelectionSnapshot,
    replacement_text: &str,
    recapture: C,
    replace: R,
) -> Result<SelectionReplacementOutcome, SelectionReplaceError>
where
    C: FnOnce() -> Result<SelectionSnapshot, SelectionCaptureError>,
    R: FnOnce(&SelectionReplacementPlan) -> Result<(), String>,
{
    let current = recapture().map_err(SelectionReplaceError::CurrentSelectionUnavailable)?;
    let plan = plan_selected_text_replacement(captured, &current, replacement_text)?;

    match replace(&plan) {
        Ok(()) => Ok(SelectionReplacementOutcome::Replaced(plan)),
        Err(error) => Ok(SelectionReplacementOutcome::Recoverable(
            recovery_for_failed_replacement(&plan, SelectionReplacementFailure::PasteFailed(error)),
        )),
    }
}

pub fn recovery_for_failed_replacement(
    plan: &SelectionReplacementPlan,
    failure: SelectionReplacementFailure,
) -> SelectionReplacementRecovery {
    let reason = match failure {
        SelectionReplacementFailure::PasteFailed(error) => format!("paste failed: {error}"),
    };

    SelectionReplacementRecovery {
        copy_text: plan.replacement_text.clone(),
        original_selected_text: plan.original_selected_text.clone(),
        reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selection(target_id: &str, focused_text: &str, selected_text: &str) -> SelectionSnapshot {
        SelectionSnapshot::from_focused_snapshot(
            FocusedTextSnapshot {
                target_id: target_id.to_string(),
                text: focused_text.to_string(),
            },
            selected_text.to_string(),
        )
    }

    #[test]
    fn rejects_replacement_when_no_text_is_selected() {
        let captured = selection("notepad|editor", "alpha beta", "");
        let current = captured.clone();

        let result = plan_selected_text_replacement(&captured, &current, "replacement");

        assert_eq!(result, Err(SelectionReplaceError::NoSelection));
    }

    #[test]
    fn rejects_replacement_when_target_changes_before_insert() {
        let captured = selection("notepad|editor", "alpha beta", "beta");
        let current = selection("browser|textarea", "alpha beta", "beta");

        let result = plan_selected_text_replacement(&captured, &current, "replacement");

        assert_eq!(
            result,
            Err(SelectionReplaceError::TargetChanged {
                expected_target_id: "notepad|editor".to_string(),
                actual_target_id: "browser|textarea".to_string(),
            })
        );
    }

    #[test]
    fn rejects_replacement_when_selection_changes_before_insert() {
        let captured = selection("notepad|editor", "alpha beta gamma", "beta");
        let current = selection("notepad|editor", "alpha beta gamma", "gamma");

        let result = plan_selected_text_replacement(&captured, &current, "replacement");

        assert_eq!(
            result,
            Err(SelectionReplaceError::SelectionChanged {
                expected_len: 4,
                actual_len: 5,
            })
        );
    }

    #[test]
    fn rejects_replacement_when_focused_text_changes_before_insert() {
        let captured = selection("notepad|editor", "alpha beta gamma", "beta");
        let current = selection("notepad|editor", "alpha beta delta", "beta");

        let result = plan_selected_text_replacement(&captured, &current, "replacement");

        assert_eq!(
            result,
            Err(SelectionReplaceError::FocusedTextChanged {
                expected_len: 16,
                actual_len: 16,
            })
        );
    }

    #[test]
    fn rejects_replacement_when_selected_text_has_multiple_possible_locations() {
        let captured = selection("notepad|editor", "alpha beta gamma beta", "beta");
        let current = captured.clone();

        let result = plan_selected_text_replacement(&captured, &current, "replacement");

        assert_eq!(
            result,
            Err(SelectionReplaceError::AmbiguousSelection {
                occurrence_count: 2,
            })
        );
    }

    #[test]
    fn rejects_replacement_when_selected_text_is_not_anchored_in_focused_text() {
        let captured = selection("notepad|editor", "alpha gamma", "beta");
        let current = captured.clone();

        let result = plan_selected_text_replacement(&captured, &current, "replacement");

        assert_eq!(
            result,
            Err(SelectionReplaceError::AmbiguousSelection {
                occurrence_count: 0,
            })
        );
    }

    #[test]
    fn rejects_replacement_when_selected_text_has_overlapping_matches() {
        let captured = selection("notepad|editor", "aaa", "aa");
        let current = captured.clone();

        let result = plan_selected_text_replacement(&captured, &current, "replacement");

        assert_eq!(
            result,
            Err(SelectionReplaceError::AmbiguousSelection {
                occurrence_count: 2,
            })
        );
    }

    #[test]
    fn clipboard_restore_failure_keeps_replacement_recoverable() {
        let captured = selection("notepad|editor", "alpha beta", "beta");
        let plan =
            plan_selected_text_replacement(&captured, &captured, "BETA").expect("valid plan");

        let recovery = recovery_for_failed_replacement(
            &plan,
            SelectionReplacementFailure::PasteFailed(
                "clipboard restore failed: restore denied".to_string(),
            ),
        );

        assert_eq!(recovery.copy_text, "BETA");
        assert_eq!(recovery.original_selected_text, "beta");
        assert!(recovery.reason.contains("restore denied"));
    }

    #[test]
    fn stable_target_and_selection_returns_replacement_plan() {
        let captured = selection("notepad|editor", "alpha beta", "beta");
        let current = selection("notepad|editor", "alpha beta", "beta");

        let plan = plan_selected_text_replacement(&captured, &current, "BETA").expect("valid plan");

        assert_eq!(
            plan,
            SelectionReplacementPlan {
                target_id: "notepad|editor".to_string(),
                original_selected_text: "beta".to_string(),
                replacement_text: "BETA".to_string(),
            }
        );
    }

    #[test]
    fn focused_snapshot_without_selected_text_is_not_a_transform_target() {
        let snapshot = FocusedTextSelectionSnapshot {
            target_id: "notepad|editor".to_string(),
            text: "alpha beta".to_string(),
            selection: FocusedTextSelection::Empty,
        };

        let result = selection_snapshot_from_focused_text_selection(snapshot);

        assert_eq!(result, Err(SelectionCaptureError::NoSelection));
    }

    #[test]
    fn focused_snapshot_with_unsupported_selection_is_unavailable() {
        let snapshot = FocusedTextSelectionSnapshot {
            target_id: "notepad|editor".to_string(),
            text: "alpha beta".to_string(),
            selection: FocusedTextSelection::Unsupported("selection unsupported".to_string()),
        };

        let result = selection_snapshot_from_focused_text_selection(snapshot);

        assert_eq!(
            result,
            Err(SelectionCaptureError::Unavailable(
                "selection unsupported".to_string()
            ))
        );
    }

    #[test]
    fn secure_capture_errors_have_stable_reason_codes() {
        assert_eq!(
            SelectionCaptureError::SecureField.reason_code(),
            SECURE_FIELD_REASON_CODE
        );
        assert_eq!(
            SelectionCaptureError::SecureCheckError.reason_code(),
            SECURE_CHECK_ERROR_REASON_CODE
        );
        assert_eq!(
            SelectionCaptureError::SecureField.to_string(),
            "secure_field"
        );
        assert_eq!(
            SelectionCaptureError::SecureCheckError.to_string(),
            "secure_check_error"
        );
    }

    #[test]
    fn secure_capture_policy_propagates_before_snapshot_conversion() {
        for capture_error in [
            SelectionCaptureError::SecureField,
            SelectionCaptureError::SecureCheckError,
        ] {
            let result = capture_current_selection_snapshot_with(|| Err(capture_error.clone()));

            assert_eq!(result, Err(capture_error));
        }
    }

    #[test]
    fn capture_current_selection_snapshot_routes_through_selection_module() {
        let result = capture_current_selection_snapshot_with(|| {
            Ok(FocusedTextSelectionSnapshot {
                target_id: "notepad|editor".to_string(),
                text: "alpha beta".to_string(),
                selection: FocusedTextSelection::Selected("beta".to_string()),
            })
        })
        .expect("selected text should be captured");

        assert_eq!(
            result,
            SelectionSnapshot {
                target_id: "notepad|editor".to_string(),
                focused_text: "alpha beta".to_string(),
                selected_text: "beta".to_string(),
            }
        );
    }

    #[test]
    fn replacement_does_not_run_when_target_changes() {
        let captured = selection("notepad|editor", "alpha beta", "beta");
        let current = selection("browser|textarea", "alpha beta", "beta");
        let mut called_replace = false;

        let result = replace_selected_text_after_recapture(
            &captured,
            "BETA",
            || Ok(current),
            |_| {
                called_replace = true;
                Ok(())
            },
        );

        assert_eq!(
            result,
            Err(SelectionReplaceError::TargetChanged {
                expected_target_id: "notepad|editor".to_string(),
                actual_target_id: "browser|textarea".to_string(),
            })
        );
        assert!(!called_replace);
    }

    #[test]
    fn replacement_failure_returns_recoverable_text() {
        let captured = selection("notepad|editor", "alpha beta", "beta");

        let result = replace_selected_text_after_recapture(
            &captured,
            "BETA",
            || Ok(captured.clone()),
            |_| Err("clipboard busy".to_string()),
        )
        .expect("replacement failure should produce recovery");

        match result {
            SelectionReplacementOutcome::Recoverable(recovery) => {
                assert_eq!(recovery.copy_text, "BETA");
                assert_eq!(recovery.original_selected_text, "beta");
                assert!(recovery.reason.contains("clipboard busy"));
            }
            SelectionReplacementOutcome::Replaced(_) => panic!("replacement should not succeed"),
        }
    }
}
