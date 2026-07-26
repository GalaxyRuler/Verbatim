use crate::adaptive::types::{InsertionMethod, InsertionReceipt};
use serde::Serialize;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

const STATUS_PATH_ENV: &str = "VERBATIM_SMOKE_STATUS_PATH";
const BARRIER_DIRECTORY_ENV: &str = "VERBATIM_SMOKE_BARRIER_DIR";
const BARRIER_STAGES_ENV: &str = "VERBATIM_SMOKE_BARRIER_STAGES";
const BARRIER_TIMEOUT_ENV: &str = "VERBATIM_SMOKE_BARRIER_TIMEOUT_MS";
const INSERTION_RECEIPT_PATH_ENV: &str = "VERBATIM_SMOKE_INSERTION_RECEIPT_PATH";
const INSERTION_CASE_ENV: &str = "VERBATIM_SMOKE_INSERTION_CASE";
const MODEL_DIRECTORY_ENV: &str = "VERBATIM_SMOKE_MODEL_DIR";

#[derive(Serialize)]
struct NativeSmokeInsertionReceipt<'a> {
    schema_version: u8,
    case: String,
    attempted: bool,
    succeeded: bool,
    method: &'a InsertionMethod,
    target_verified: bool,
    error: &'a Option<String>,
}

#[derive(Serialize)]
struct NativeSmokeBarrierReady<'a> {
    schema_version: u8,
    stage: &'a str,
}

/// Native-smoke file overrides are only active when the normal status contract
/// is present, keeping these test controls inert in ordinary application runs.
pub(crate) fn is_enabled() -> bool {
    std::env::var_os(STATUS_PATH_ENV).is_some()
}

pub(crate) fn model_directory_override() -> Option<PathBuf> {
    absolute_path_from_env(MODEL_DIRECTORY_ENV)
}

pub(crate) fn wait_for_barrier(stage: &str) -> Result<(), String> {
    let stages = std::env::var(BARRIER_STAGES_ENV).unwrap_or_default();
    if !stages
        .split(',')
        .map(str::trim)
        .any(|configured_stage| configured_stage == stage)
    {
        return Ok(());
    }
    let Some(directory) = absolute_path_from_env(BARRIER_DIRECTORY_ENV) else {
        return Ok(());
    };
    if !stage
        .chars()
        .all(|character| character.is_ascii_lowercase() || character == '_')
    {
        return Err(format!("invalid native smoke barrier stage: {stage}"));
    }

    fs::create_dir_all(&directory).map_err(|error| {
        format!(
            "create native smoke barrier directory {}: {error}",
            directory.display()
        )
    })?;

    let ready_path = directory.join(format!("{stage}.ready.json"));
    let continue_path = directory.join(format!("{stage}.continue"));
    let _ = fs::remove_file(&ready_path);
    let _ = fs::remove_file(&continue_path);
    let ready = serde_json::to_vec(&NativeSmokeBarrierReady {
        schema_version: 1,
        stage,
    })
    .map_err(|error| format!("serialize native smoke barrier: {error}"))?;
    fs::write(&ready_path, ready).map_err(|error| {
        format!(
            "write native smoke barrier {}: {error}",
            ready_path.display()
        )
    })?;

    let timeout = barrier_timeout();
    let started = Instant::now();
    while !continue_path.exists() {
        if started.elapsed() >= timeout {
            return Err(format!(
                "native smoke barrier {stage} timed out after {}ms",
                timeout.as_millis()
            ));
        }
        thread::sleep(Duration::from_millis(10));
    }
    let _ = fs::remove_file(&continue_path);
    Ok(())
}

pub(crate) fn record_insertion_receipt(receipt: &InsertionReceipt) {
    let Some(path) = absolute_path_from_env(INSERTION_RECEIPT_PATH_ENV) else {
        return;
    };
    let record = NativeSmokeInsertionReceipt {
        schema_version: 1,
        case: std::env::var(INSERTION_CASE_ENV)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "unspecified".to_string()),
        attempted: receipt.attempted,
        succeeded: receipt.succeeded,
        method: &receipt.method,
        target_verified: receipt.target_verified,
        error: &receipt.error,
    };
    let Ok(serialized) = serde_json::to_vec(&record) else {
        return;
    };
    let Some(parent) = path.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() {
        return;
    }

    let result = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut file| {
            file.write_all(&serialized)?;
            file.write_all(b"\n")
        });
    if let Err(error) = result {
        log::warn!(
            "Failed to write native smoke insertion receipt {}: {error}",
            path.display()
        );
    }
}

fn absolute_path_from_env(name: &str) -> Option<PathBuf> {
    if !is_enabled() {
        return None;
    }
    let value = std::env::var_os(name)?;
    if value.is_empty() {
        return None;
    }
    let path = PathBuf::from(value);
    path.is_absolute().then_some(path)
}

fn barrier_timeout() -> Duration {
    const DEFAULT_TIMEOUT_MS: u64 = 10_000;
    const MAX_TIMEOUT_MS: u64 = 30_000;

    match std::env::var(BARRIER_TIMEOUT_ENV) {
        Ok(value) => match value.parse::<u64>() {
            Ok(timeout_ms) if (100..=MAX_TIMEOUT_MS).contains(&timeout_ms) => {
                Duration::from_millis(timeout_ms)
            }
            _ => Duration::from_millis(DEFAULT_TIMEOUT_MS),
        },
        Err(_) => Duration::from_millis(DEFAULT_TIMEOUT_MS),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insertion_receipt_serialization_excludes_transcript_text() {
        let receipt = InsertionReceipt {
            attempted: true,
            succeeded: false,
            method: InsertionMethod::Clipboard,
            target_verified: true,
            error: Some("clipboard changed before paste".to_string()),
        };
        let record = NativeSmokeInsertionReceipt {
            schema_version: 1,
            case: "clipboard_mutation".to_string(),
            attempted: receipt.attempted,
            succeeded: receipt.succeeded,
            method: &receipt.method,
            target_verified: receipt.target_verified,
            error: &receipt.error,
        };

        let serialized = serde_json::to_string(&record).unwrap();

        assert!(serialized.contains("clipboard_mutation"));
        assert!(!serialized.contains("text"));
        assert!(!serialized.contains("transcript"));
    }
}
