use crate::adaptive::types::{InsertionMethod, InsertionReceipt};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InsertionKind {
    Adaptive,
    Classic,
    PasteLastTranscript,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InsertionPasteRequest {
    pub text: String,
    pub target_verified: bool,
    pub auto_learn_eligible: bool,
}

impl InsertionAttempt {
    pub fn adaptive_guard_blocked() -> Self {
        Self {
            kind: InsertionKind::Adaptive,
            block: Some(InsertionBlock::LanguageGuard),
            text: None,
            auto_learn_eligible: false,
        }
    }

    pub fn adaptive_target_changed() -> Self {
        Self {
            kind: InsertionKind::Adaptive,
            block: Some(InsertionBlock::TargetChanged),
            text: None,
            auto_learn_eligible: false,
        }
    }

    pub fn adaptive_ready(text: impl Into<String>) -> Self {
        Self {
            kind: InsertionKind::Adaptive,
            block: None,
            text: Some(text.into()),
            auto_learn_eligible: true,
        }
    }

    pub fn classic_guard_blocked() -> Self {
        Self {
            kind: InsertionKind::Classic,
            block: Some(InsertionBlock::LanguageGuard),
            text: None,
            auto_learn_eligible: false,
        }
    }

    pub fn classic_ready(text: impl Into<String>) -> Self {
        Self {
            kind: InsertionKind::Classic,
            block: None,
            text: Some(text.into()),
            auto_learn_eligible: true,
        }
    }

    pub fn paste_last_transcript(text: impl Into<String>) -> Self {
        Self {
            kind: InsertionKind::PasteLastTranscript,
            block: None,
            text: Some(text.into()),
            auto_learn_eligible: false,
        }
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
        attempt.auto_learn_eligible,
    ) {
        (InsertionKind::Adaptive, Some(InsertionBlock::LanguageGuard), _, _) => InsertionOutcome {
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
        },
        (InsertionKind::Adaptive, Some(InsertionBlock::TargetChanged), _, _) => InsertionOutcome {
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
        },
        (InsertionKind::Classic, Some(InsertionBlock::LanguageGuard), _, _) => InsertionOutcome {
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
        },
        (InsertionKind::Adaptive, None, Some(text), auto_learn_eligible) => {
            resolve_ready_insertion(
                text,
                true,
                auto_learn_eligible,
                "adaptive paste failure",
                paste,
            )
        }
        (InsertionKind::Classic, None, Some(text), auto_learn_eligible) => {
            resolve_ready_insertion(text, true, auto_learn_eligible, "paste failure", paste)
        }
        (InsertionKind::PasteLastTranscript, None, Some(text), auto_learn_eligible) => {
            resolve_ready_insertion(
                text,
                true,
                auto_learn_eligible,
                "paste last transcript failure",
                paste,
            )
        }
        _ => unreachable!("invalid insertion attempt"),
    }
}

fn resolve_ready_insertion<F>(
    text: String,
    target_verified: bool,
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
        auto_learn_eligible,
    });
    let recovery_copy = if receipt.succeeded {
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
}
