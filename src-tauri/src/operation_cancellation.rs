use crate::providers::CancellationToken;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

#[derive(Debug, Default)]
pub(crate) struct OperationCancellationState {
    current_operation_id: AtomicU64,
    cancelled_operation_id: AtomicU64,
    current_provider_token: Mutex<Option<(u64, CancellationToken)>>,
}

#[derive(Clone, Debug)]
pub(crate) struct OperationToken {
    id: u64,
    provider_cancellation: CancellationToken,
}

impl OperationToken {
    pub(crate) fn provider_cancellation(&self) -> CancellationToken {
        self.provider_cancellation.clone()
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.provider_cancellation.is_cancelled()
    }
}

impl OperationCancellationState {
    pub(crate) fn begin_operation(&self) -> OperationToken {
        let id = self.current_operation_id.fetch_add(1, Ordering::Relaxed) + 1;
        let provider_cancellation = CancellationToken::default();
        let token = OperationToken {
            id,
            provider_cancellation,
        };

        let mut current_provider_token = self
            .current_provider_token
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        *current_provider_token = Some((id, token.provider_cancellation.clone()));

        token
    }

    pub(crate) fn current_token(&self) -> Option<OperationToken> {
        let current_provider_token = self
            .current_provider_token
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        current_provider_token
            .as_ref()
            .map(|(id, provider_cancellation)| OperationToken {
                id: *id,
                provider_cancellation: provider_cancellation.clone(),
            })
    }

    pub(crate) fn cancel_current_operation(&self) {
        let current_operation_id = self.current_operation_id.load(Ordering::Relaxed);
        update_cancelled_operation_id(&self.cancelled_operation_id, current_operation_id);

        let current_provider_token = self
            .current_provider_token
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        if let Some((id, provider_cancellation)) = current_provider_token.as_ref() {
            if *id == current_operation_id {
                provider_cancellation.cancel();
            }
        }
    }

    pub(crate) fn is_cancelled(&self, token: &OperationToken) -> bool {
        token.provider_cancellation.is_cancelled()
            || self.cancelled_operation_id.load(Ordering::Relaxed) >= token.id
    }
}

fn update_cancelled_operation_id(cancelled_operation_id: &AtomicU64, candidate: u64) {
    let mut current = cancelled_operation_id.load(Ordering::Relaxed);
    while candidate > current {
        match cancelled_operation_id.compare_exchange_weak(
            current,
            candidate,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(next_current) => current = next_current,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_marks_current_operation_and_provider_token() {
        let state = OperationCancellationState::default();
        let token = state.begin_operation();

        assert!(!state.is_cancelled(&token));
        assert!(!token.provider_cancellation().is_cancelled());

        state.cancel_current_operation();

        assert!(state.is_cancelled(&token));
        assert!(token.provider_cancellation().is_cancelled());
    }

    #[test]
    fn operation_started_after_cancel_is_not_cancelled() {
        let state = OperationCancellationState::default();
        let first = state.begin_operation();
        state.cancel_current_operation();

        let second = state.begin_operation();

        assert!(state.is_cancelled(&first));
        assert!(!state.is_cancelled(&second));
        assert!(!second.provider_cancellation().is_cancelled());
    }
}
