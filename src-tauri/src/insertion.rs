use crate::adaptive::types::{InsertionMethod, InsertionReceipt};
use serde::Serialize;

pub(crate) const CLIPBOARD_CHANGED_BEFORE_PASTE: &str = "clipboard changed before paste";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InsertionKind {
    Adaptive,
    Classic,
    PasteLastTranscript,
    TransformReplacement,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InsertionBlock {
    LanguageGuard,
    TargetChanged,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InsertionAttempt {
    kind: InsertionKind,
    block: Option<InsertionBlock>,
    text: Option<String>,
    expected_target: Option<String>,
    auto_learn_eligible: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryCopy {
    pub text: String,
    pub reason: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InsertionOutcome {
    pub receipt: InsertionReceipt,
    pub recovery_copy: Option<RecoveryCopy>,
    pub auto_learn_eligible: bool,
    pub emit_paste_error: bool,
    pub emit_inserted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PasteRecoveryReason {
    PasteFailure,
    TargetChanged,
    LanguageGuard,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PasteRecoveryEvent {
    pub reason: PasteRecoveryReason,
    pub copied: bool,
    pub paste_here_available: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InsertionPasteRequest {
    pub text: String,
    pub target_verified: bool,
    pub expected_target: Option<String>,
    pub auto_learn_eligible: bool,
}

pub struct InsertionTransaction<P> {
    paste: P,
}

impl<P> InsertionTransaction<P>
where
    P: FnMut(InsertionPasteRequest) -> InsertionReceipt,
{
    pub fn new(paste: P) -> Self {
        Self { paste }
    }

    pub fn run(&mut self, attempt: InsertionAttempt) -> InsertionOutcome {
        resolve_insertion_attempt(attempt, |request| (self.paste)(request))
    }
}

impl InsertionOutcome {
    pub fn paste_recovery_event(&self) -> Option<PasteRecoveryEvent> {
        if !self.emit_paste_error {
            return None;
        }

        let error = self.receipt.error.as_deref();
        let reason = match error {
            Some("target changed before insertion") => PasteRecoveryReason::TargetChanged,
            Some("language guard blocked paste") => PasteRecoveryReason::LanguageGuard,
            _ => PasteRecoveryReason::PasteFailure,
        };

        Some(PasteRecoveryEvent {
            paste_here_available: matches!(reason, PasteRecoveryReason::TargetChanged),
            copied: self.recovery_copy.is_some()
                || matches!(reason, PasteRecoveryReason::LanguageGuard),
            reason,
        })
    }
}

impl InsertionAttempt {
    pub fn adaptive_guard_blocked() -> Self {
        Self {
            kind: InsertionKind::Adaptive,
            block: Some(InsertionBlock::LanguageGuard),
            text: None,
            expected_target: None,
            auto_learn_eligible: false,
        }
    }

    pub fn adaptive_target_changed() -> Self {
        Self {
            kind: InsertionKind::Adaptive,
            block: Some(InsertionBlock::TargetChanged),
            text: None,
            expected_target: None,
            auto_learn_eligible: false,
        }
    }

    pub fn adaptive_ready(text: impl Into<String>) -> Self {
        Self {
            kind: InsertionKind::Adaptive,
            block: None,
            text: Some(text.into()),
            expected_target: None,
            auto_learn_eligible: true,
        }
    }

    pub fn classic_guard_blocked() -> Self {
        Self {
            kind: InsertionKind::Classic,
            block: Some(InsertionBlock::LanguageGuard),
            text: None,
            expected_target: None,
            auto_learn_eligible: false,
        }
    }

    pub fn classic_target_changed() -> Self {
        Self {
            kind: InsertionKind::Classic,
            block: Some(InsertionBlock::TargetChanged),
            text: None,
            expected_target: None,
            auto_learn_eligible: false,
        }
    }

    pub fn classic_ready(text: impl Into<String>) -> Self {
        Self {
            kind: InsertionKind::Classic,
            block: None,
            text: Some(text.into()),
            expected_target: None,
            auto_learn_eligible: true,
        }
    }

    pub fn paste_last_transcript(text: impl Into<String>) -> Self {
        Self {
            kind: InsertionKind::PasteLastTranscript,
            block: None,
            text: Some(text.into()),
            expected_target: None,
            auto_learn_eligible: false,
        }
    }

    pub fn transform_replacement(text: impl Into<String>) -> Self {
        Self {
            kind: InsertionKind::TransformReplacement,
            block: None,
            text: Some(text.into()),
            expected_target: None,
            auto_learn_eligible: false,
        }
    }

    pub fn with_expected_target(mut self, expected_target: Option<String>) -> Self {
        self.expected_target = expected_target;
        self
    }
}

pub fn resolve_insertion_attempt<F>(attempt: InsertionAttempt, paste: F) -> InsertionOutcome
where
    F: FnOnce(InsertionPasteRequest) -> InsertionReceipt,
{
    match (
        attempt.kind,
        attempt.block,
        attempt.text,
        attempt.expected_target,
        attempt.auto_learn_eligible,
    ) {
        (InsertionKind::Adaptive, Some(InsertionBlock::LanguageGuard), _, _, _) => {
            InsertionOutcome {
                receipt: InsertionReceipt {
                    attempted: false,
                    succeeded: false,
                    method: InsertionMethod::None,
                    target_verified: true,
                    error: Some("language guard blocked paste".to_string()),
                },
                recovery_copy: None,
                auto_learn_eligible: false,
                emit_paste_error: true,
                emit_inserted: false,
            }
        }
        (InsertionKind::Adaptive, Some(InsertionBlock::TargetChanged), _, _, _) => {
            InsertionOutcome {
                receipt: InsertionReceipt {
                    attempted: false,
                    succeeded: false,
                    method: InsertionMethod::None,
                    target_verified: false,
                    error: Some("target changed before insertion".to_string()),
                },
                recovery_copy: None,
                auto_learn_eligible: false,
                emit_paste_error: true,
                emit_inserted: false,
            }
        }
        (InsertionKind::Classic, Some(InsertionBlock::LanguageGuard), _, _, _) => {
            InsertionOutcome {
                receipt: InsertionReceipt {
                    attempted: false,
                    succeeded: false,
                    method: InsertionMethod::None,
                    target_verified: true,
                    error: Some("language guard blocked paste".to_string()),
                },
                recovery_copy: None,
                auto_learn_eligible: false,
                emit_paste_error: false,
                emit_inserted: false,
            }
        }
        (InsertionKind::Classic, Some(InsertionBlock::TargetChanged), _, _, _) => {
            InsertionOutcome {
                receipt: InsertionReceipt {
                    attempted: false,
                    succeeded: false,
                    method: InsertionMethod::None,
                    target_verified: false,
                    error: Some("target changed before insertion".to_string()),
                },
                recovery_copy: None,
                auto_learn_eligible: false,
                emit_paste_error: true,
                emit_inserted: false,
            }
        }
        (InsertionKind::Adaptive, None, Some(text), expected_target, auto_learn_eligible) => {
            resolve_ready_insertion(
                text,
                true,
                expected_target,
                auto_learn_eligible,
                "adaptive paste failure",
                paste,
            )
        }
        (InsertionKind::Classic, None, Some(text), expected_target, auto_learn_eligible) => {
            resolve_ready_insertion(
                text,
                true,
                expected_target,
                auto_learn_eligible,
                "paste failure",
                paste,
            )
        }
        (
            InsertionKind::PasteLastTranscript,
            None,
            Some(text),
            expected_target,
            auto_learn_eligible,
        ) => resolve_ready_insertion(
            text,
            true,
            expected_target,
            auto_learn_eligible,
            "paste last transcript failure",
            paste,
        ),
        (
            InsertionKind::TransformReplacement,
            None,
            Some(text),
            expected_target,
            auto_learn_eligible,
        ) => resolve_ready_insertion(
            text,
            true,
            expected_target,
            auto_learn_eligible,
            "transform replacement failure",
            paste,
        ),
        _ => unreachable!("invalid insertion attempt"),
    }
}

fn resolve_ready_insertion<F>(
    text: String,
    target_verified: bool,
    expected_target: Option<String>,
    auto_learn_eligible: bool,
    recovery_reason: &'static str,
    paste: F,
) -> InsertionOutcome
where
    F: FnOnce(InsertionPasteRequest) -> InsertionReceipt,
{
    let receipt = paste(InsertionPasteRequest {
        text: text.clone(),
        target_verified,
        expected_target,
        auto_learn_eligible,
    });
    // A paste session can detect that another application changed the clipboard
    // after Verbatim wrote its own payload. The newer clipboard value belongs to
    // that application, so generic failure recovery must not overwrite it.
    let recovery_copy =
        if receipt.succeeded || receipt.error.as_deref() == Some(CLIPBOARD_CHANGED_BEFORE_PASTE) {
            None
        } else {
            Some(RecoveryCopy {
                text,
                reason: recovery_reason,
            })
        };

    InsertionOutcome {
        emit_paste_error: !receipt.succeeded,
        emit_inserted: receipt.succeeded,
        auto_learn_eligible,
        receipt,
        recovery_copy,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn success_receipt(target_verified: bool) -> InsertionReceipt {
        InsertionReceipt {
            attempted: true,
            succeeded: true,
            method: InsertionMethod::Clipboard,
            target_verified,
            error: None,
        }
    }

    fn failed_receipt(target_verified: bool) -> InsertionReceipt {
        InsertionReceipt {
            attempted: true,
            succeeded: false,
            method: InsertionMethod::Clipboard,
            target_verified,
            error: Some("paste failed".to_string()),
        }
    }

    #[test]
    fn adaptive_target_changed_is_not_attempted() {
        let outcome =
            resolve_insertion_attempt(InsertionAttempt::adaptive_target_changed(), |_| {
                panic!("changed target must not paste")
            });

        assert!(!outcome.receipt.attempted);
        assert!(!outcome.receipt.succeeded);
        assert_eq!(outcome.receipt.method, InsertionMethod::None);
        assert!(!outcome.receipt.target_verified);
        assert_eq!(
            outcome.receipt.error.as_deref(),
            Some("target changed before insertion")
        );
        assert!(outcome.emit_paste_error);
        assert!(!outcome.emit_inserted);
        assert!(!outcome.auto_learn_eligible);
        assert!(outcome.recovery_copy.is_none());
        assert_eq!(
            outcome.paste_recovery_event(),
            Some(PasteRecoveryEvent {
                reason: PasteRecoveryReason::TargetChanged,
                copied: false,
                paste_here_available: true,
            })
        );
    }

    #[test]
    fn classic_target_changed_is_not_attempted() {
        let outcome = resolve_insertion_attempt(InsertionAttempt::classic_target_changed(), |_| {
            panic!("changed target must not paste")
        });

        assert!(!outcome.receipt.attempted);
        assert!(!outcome.receipt.succeeded);
        assert_eq!(outcome.receipt.method, InsertionMethod::None);
        assert!(!outcome.receipt.target_verified);
        assert_eq!(
            outcome.receipt.error.as_deref(),
            Some("target changed before insertion")
        );
        assert!(outcome.emit_paste_error);
        assert!(!outcome.emit_inserted);
        assert!(!outcome.auto_learn_eligible);
        assert!(outcome.recovery_copy.is_none());
        assert_eq!(
            outcome.paste_recovery_event(),
            Some(PasteRecoveryEvent {
                reason: PasteRecoveryReason::TargetChanged,
                copied: false,
                paste_here_available: true,
            })
        );
    }

    #[test]
    fn adaptive_ready_success_emits_inserted_without_recovery_copy() {
        let outcome =
            resolve_insertion_attempt(InsertionAttempt::adaptive_ready("hello"), |request| {
                assert_eq!(request.text, "hello");
                assert!(request.auto_learn_eligible);
                success_receipt(request.target_verified)
            });

        assert!(outcome.receipt.succeeded);
        assert!(outcome.emit_inserted);
        assert!(!outcome.emit_paste_error);
        assert!(outcome.auto_learn_eligible);
        assert!(outcome.recovery_copy.is_none());
    }

    #[test]
    fn adaptive_ready_failure_keeps_recovery_copy() {
        let outcome =
            resolve_insertion_attempt(InsertionAttempt::adaptive_ready("recover me"), |request| {
                assert_eq!(request.text, "recover me");
                assert!(request.auto_learn_eligible);
                failed_receipt(request.target_verified)
            });

        assert!(!outcome.receipt.succeeded);
        assert!(outcome.emit_paste_error);
        assert!(!outcome.emit_inserted);
        assert!(outcome.auto_learn_eligible);
        assert_eq!(
            outcome.recovery_copy,
            Some(RecoveryCopy {
                text: "recover me".to_string(),
                reason: "adaptive paste failure",
            })
        );
        assert_eq!(
            outcome.paste_recovery_event(),
            Some(PasteRecoveryEvent {
                reason: PasteRecoveryReason::PasteFailure,
                copied: true,
                paste_here_available: false,
            })
        );
    }

    #[test]
    fn classic_guard_block_is_not_attempted_and_preserves_existing_no_paste_error_behavior() {
        let outcome = resolve_insertion_attempt(InsertionAttempt::classic_guard_blocked(), |_| {
            panic!("guarded insertion must not paste")
        });

        assert!(!outcome.receipt.attempted);
        assert!(!outcome.receipt.succeeded);
        assert_eq!(outcome.receipt.method, InsertionMethod::None);
        assert!(outcome.receipt.target_verified);
        assert!(!outcome.emit_paste_error);
        assert!(!outcome.emit_inserted);
        assert!(!outcome.auto_learn_eligible);
        assert!(outcome.recovery_copy.is_none());
        assert_eq!(outcome.paste_recovery_event(), None);
    }

    #[test]
    fn classic_ready_failure_keeps_recovery_copy() {
        let outcome =
            resolve_insertion_attempt(InsertionAttempt::classic_ready("classic"), |request| {
                assert!(request.auto_learn_eligible);
                failed_receipt(request.target_verified)
            });

        assert_eq!(
            outcome.recovery_copy,
            Some(RecoveryCopy {
                text: "classic".to_string(),
                reason: "paste failure",
            })
        );
        assert!(outcome.auto_learn_eligible);
        assert!(outcome.emit_paste_error);
    }

    #[test]
    fn clipboard_mutation_failure_does_not_overwrite_newer_clipboard_content() {
        let outcome = resolve_insertion_attempt(
            InsertionAttempt::classic_ready("recover me without clobbering"),
            |request| InsertionReceipt {
                attempted: true,
                succeeded: false,
                method: InsertionMethod::Clipboard,
                target_verified: request.target_verified,
                error: Some("clipboard changed before paste".to_string()),
            },
        );

        assert!(outcome.emit_paste_error);
        assert!(outcome.recovery_copy.is_none());
        assert_eq!(
            outcome.paste_recovery_event(),
            Some(PasteRecoveryEvent {
                reason: PasteRecoveryReason::PasteFailure,
                copied: false,
                paste_here_available: false,
            })
        );
    }

    #[test]
    fn paste_last_failure_keeps_recovery_copy() {
        let outcome =
            resolve_insertion_attempt(InsertionAttempt::paste_last_transcript("last"), |request| {
                assert!(!request.auto_learn_eligible);
                failed_receipt(request.target_verified)
            });

        assert_eq!(
            outcome.recovery_copy,
            Some(RecoveryCopy {
                text: "last".to_string(),
                reason: "paste last transcript failure",
            })
        );
        assert!(!outcome.auto_learn_eligible);
        assert!(outcome.emit_paste_error);
    }

    #[test]
    fn transform_replacement_success_uses_shared_transaction_without_auto_learn() {
        let outcome = resolve_insertion_attempt(
            InsertionAttempt::transform_replacement("polished"),
            |request| {
                assert_eq!(request.text, "polished");
                assert!(request.target_verified);
                assert!(!request.auto_learn_eligible);
                success_receipt(request.target_verified)
            },
        );

        assert!(outcome.receipt.succeeded);
        assert!(outcome.emit_inserted);
        assert!(!outcome.auto_learn_eligible);
        assert!(outcome.recovery_copy.is_none());
    }

    #[test]
    fn transform_replacement_failure_keeps_recovery_copy() {
        let outcome = resolve_insertion_attempt(
            InsertionAttempt::transform_replacement("copy me"),
            |request| {
                assert!(!request.auto_learn_eligible);
                failed_receipt(request.target_verified)
            },
        );

        assert!(!outcome.receipt.succeeded);
        assert_eq!(
            outcome.recovery_copy,
            Some(RecoveryCopy {
                text: "copy me".to_string(),
                reason: "transform replacement failure",
            })
        );
        assert!(!outcome.auto_learn_eligible);
    }

    #[test]
    fn insertion_transaction_runs_ready_attempt_through_shared_paste_callback() {
        let mut pasted_text = Vec::new();
        let mut transaction = InsertionTransaction::new(|request: InsertionPasteRequest| {
            pasted_text.push(request.text);
            assert!(request.target_verified);
            assert!(request.auto_learn_eligible);
            success_receipt(request.target_verified)
        });

        let outcome = transaction.run(InsertionAttempt::classic_ready("transaction text"));
        drop(transaction);

        assert!(outcome.receipt.succeeded);
        assert_eq!(pasted_text, vec!["transaction text".to_string()]);
    }

    #[test]
    fn insertion_transaction_blocks_target_change_before_paste_callback() {
        let mut transaction = InsertionTransaction::new(|_request: InsertionPasteRequest| {
            panic!("target-changed transaction must not paste")
        });

        let outcome = transaction.run(InsertionAttempt::adaptive_target_changed());

        assert!(!outcome.receipt.attempted);
        assert!(!outcome.receipt.target_verified);
        assert_eq!(
            outcome.receipt.error.as_deref(),
            Some("target changed before insertion")
        );
    }
}
