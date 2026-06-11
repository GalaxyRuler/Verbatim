# Engine Provider Architecture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor Verbatim so speech models, execution providers, and translation tasks are represented separately, while preserving existing dictation behavior.

**Architecture:** Add a provider seam around the current `transcribe-rs` engines before adding new model families. `TranscriptionManager` remains the orchestrator for settings, output safety, panic recovery, and load/unload policy, while provider adapters own engine-specific load and inference logic.

**Tech Stack:** Rust/Tauri 2, `transcribe-rs`, React/TypeScript, Zustand settings store, i18next, Bun, Cargo tests.

---

## Source Context

- ADR: `docs/adr/0001-engine-provider-architecture.md`
- Current coupling hotspots:
  - `src-tauri/src/managers/transcription.rs`: `LoadedEngine` enum, model loading, engine dispatch, translation decision, panic boundary.
  - `src-tauri/src/managers/model.rs`: `EngineType`, `ModelInfo`, model download/delete/path resolution.
  - `src-tauri/src/settings.rs`: legacy `translate_to_english`, selected model/language settings.
  - `src/components/settings/TranslateToEnglish.tsx`: English-only translation UI.
  - `src/components/settings/general/ModelSettingsCard.tsx`: model capability-driven settings display.
  - `src/stores/settingsStore.ts`: settings update command map.
  - `src/bindings.ts`: generated Tauri bindings.

## File Structure Target

- Create: `src-tauri/src/providers/mod.rs`
  - Exposes provider traits, request/response types, registry, and current local provider adapter.
- Create: `src-tauri/src/providers/types.rs`
  - Defines `SpeechInput`, `SpeechTaskKind`, `TranslationTarget`, `SpeechRequest`, `SpeechResponse`, `ProviderCapabilities`, `ModelAsset`, `ModelLocator`, `CancellationToken`, and capability helper methods.
- Create: `src-tauri/src/providers/transcribe_rs.rs`
  - Owns current `LoadedEngine` enum and current `transcribe-rs` load/run match logic.
- Create: `src-tauri/src/providers/registry.rs`
  - Maps `ModelAsset` and `SpeechTaskKind` to a provider.
- Modify: `src-tauri/src/lib.rs`
  - Registers the new `providers` module.
- Modify: `src-tauri/src/managers/model.rs`
  - Adds a `ModelAsset` bridge from existing `ModelInfo` and resolved model paths.
- Modify: `src-tauri/src/managers/transcription.rs`
  - Replaces direct engine branching with provider orchestration.
- Modify: `src-tauri/src/settings.rs`
  - Adds general translation settings while migrating from `translate_to_english`.
- Modify: `src-tauri/src/shortcut/mod.rs`
  - Updates translation-setting command path.
- Modify: `src/stores/settingsStore.ts`
  - Maps new translation setting updates.
- Modify: `src/components/settings/TranslateToEnglish.tsx`
  - Rename/replace with general translation UI.
- Modify: `src/components/settings/general/ModelSettingsCard.tsx`
  - Drive translation controls from provider capabilities.
- Modify: `src/i18n/locales/en/translation.json`
  - Update English source strings. Other locale files may keep old strings until translation pass.

---

### Task 1: Characterize Current Translation and Language Behavior

**Files:**
- Modify: `src-tauri/src/managers/transcription.rs`

- [ ] **Step 1: Write failing characterization tests around pure helper boundaries**

Add tests inside the existing `#[cfg(test)] mod tests` in `src-tauri/src/managers/transcription.rs`:

```rust
#[test]
fn english_translation_requires_user_toggle_and_model_support() {
    assert!(effective_english_translation(true, true));
    assert!(!effective_english_translation(true, false));
    assert!(!effective_english_translation(false, true));
    assert!(!effective_english_translation(false, false));
}

#[test]
fn unsupported_selected_language_falls_back_to_auto() {
    assert_eq!(
        validate_selected_language("ar", &["en".to_string(), "fr".to_string()]),
        "auto"
    );
    assert_eq!(
        validate_selected_language("ar", &["en".to_string(), "ar".to_string()]),
        "ar"
    );
    assert_eq!(
        validate_selected_language("auto", &["en".to_string()]),
        "auto"
    );
}

#[test]
fn empty_supported_language_list_accepts_any_selected_language() {
    assert_eq!(validate_selected_language("ar", &[]), "ar");
}
```

- [ ] **Step 2: Run tests to verify they fail because helpers do not exist**

Run:

```powershell
$env:CARGO_TARGET_DIR='C:\t\ha'; cargo test -p verbatim english_translation_requires_user_toggle_and_model_support --lib
```

Expected: FAIL with `cannot find function effective_english_translation`.

- [ ] **Step 3: Add minimal helper functions**

Add near the existing language helper functions in `src-tauri/src/managers/transcription.rs`:

```rust
fn effective_english_translation(user_requested: bool, model_supports_translation: bool) -> bool {
    user_requested && model_supports_translation
}

fn validate_selected_language(selected_language: &str, supported_languages: &[String]) -> String {
    if selected_language == "auto"
        || supported_languages.is_empty()
        || supported_languages.contains(&selected_language.to_string())
    {
        selected_language.to_string()
    } else {
        "auto".to_string()
    }
}
```

Then replace the inline translation decision in `TranscriptionManager::transcribe`:

```rust
let effective_translate_to_english = effective_english_translation(
    settings.translate_to_english,
    current_model_info
        .as_ref()
        .map(|info| info.supports_translation)
        .unwrap_or(false),
);
```

Replace the inline language validation block with:

```rust
let validated_language = validate_selected_language(
    &settings.selected_language,
    &current_model_info
        .as_ref()
        .map(|info| info.supported_languages.clone())
        .unwrap_or_default(),
);

if validated_language == "auto" && settings.selected_language != "auto" {
    warn!(
        "Language '{}' not supported by current model, falling back to auto-detect",
        settings.selected_language
    );
}
```

- [ ] **Step 4: Run focused tests**

Run:

```powershell
$env:CARGO_TARGET_DIR='C:\t\ha'; cargo test -p verbatim effective_english_translation validate_selected_language --lib
```

Expected: PASS for the new characterization tests.

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/managers/transcription.rs
git commit -m "test: characterize current translation decisions"
```

---

### Task 2: Add Provider Core Types

**Files:**
- Create: `src-tauri/src/providers/mod.rs`
- Create: `src-tauri/src/providers/types.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Write provider type tests first**

Create `src-tauri/src/providers/types.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn speech_input_requires_exactly_one_input_kind() {
        let audio = SpeechInput::Audio(Arc::from([0.0_f32, 1.0_f32]));
        assert!(matches!(audio, SpeechInput::Audio(_)));

        let text = SpeechInput::Text("hello".to_string());
        assert!(matches!(text, SpeechInput::Text(_)));
    }

    #[test]
    fn cancellation_token_defaults_to_not_cancelled() {
        let token = CancellationToken::default();
        assert!(!token.is_cancelled());
        token.cancel();
        assert!(token.is_cancelled());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```powershell
$env:CARGO_TARGET_DIR='C:\t\ha'; cargo test -p verbatim providers::types::tests:: --lib
```

Expected: FAIL until the module is registered and types are implemented.

- [ ] **Step 3: Register the provider module**

Add to `src-tauri/src/lib.rs` near existing module declarations:

```rust
pub mod providers;
```

Create `src-tauri/src/providers/mod.rs`:

```rust
pub mod types;

pub use types::*;
```

- [ ] **Step 4: Implement provider core types**

Replace `src-tauri/src/providers/types.rs` with:

```rust
use crate::managers::model::ModelInfo;
use anyhow::Result;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub enum SpeechInput {
    Audio(Arc<[f32]>),
    Text(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpeechTaskKind {
    Transcribe,
    TranslateSpeech,
    TranslateText,
    PostProcessText,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LanguageSelection {
    Auto,
    Language(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranslationTarget {
    pub source_language: LanguageSelection,
    pub target_language: String,
}

#[derive(Clone, Debug)]
pub struct SpeechRequest {
    pub task: SpeechTaskKind,
    pub input: SpeechInput,
    pub translation: Option<TranslationTarget>,
    pub language_shortlist: Vec<String>,
    pub custom_words: Vec<String>,
    pub cancellation: CancellationToken,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpeechResponse {
    pub text: String,
    pub detected_language: Option<String>,
    pub translated: bool,
    pub provider_id: &'static str,
    pub model_id: String,
}

#[derive(Clone, Debug)]
pub struct ModelAsset {
    pub id: String,
    pub locator: ModelLocator,
    pub metadata: ModelInfo,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelLocator {
    File(PathBuf),
    Directory(PathBuf),
    ManagedServer {
        endpoint: String,
        health_url: Option<String>,
    },
    ExternalHttp {
        endpoint: String,
        credential_ref: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderCapabilities {
    pub tasks: Vec<SpeechTaskKind>,
    pub translation_pairs: TranslationPairSupport,
    pub streaming: StreamingSupport,
    pub lifecycle: LifecycleCost,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TranslationPairSupport {
    None,
    EnglishOnly,
    Explicit(Vec<(String, String)>),
    AnyToAny { languages: Vec<String> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StreamingSupport {
    None,
    PartialText,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LifecycleCost {
    NoLoad,
    Cheap,
    Expensive,
    SidecarProcess,
}

#[derive(Clone, Debug, Default)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

pub trait EngineProvider: Send {
    fn provider_id(&self) -> &'static str;
    fn capabilities(&self, asset: &ModelAsset) -> ProviderCapabilities;
    fn load(&mut self, asset: &ModelAsset) -> Result<()>;
    fn unload(&mut self);
    fn run(&mut self, request: SpeechRequest) -> Result<SpeechResponse>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn speech_input_requires_exactly_one_input_kind() {
        let audio = SpeechInput::Audio(Arc::from([0.0_f32, 1.0_f32]));
        assert!(matches!(audio, SpeechInput::Audio(_)));

        let text = SpeechInput::Text("hello".to_string());
        assert!(matches!(text, SpeechInput::Text(_)));
    }

    #[test]
    fn cancellation_token_defaults_to_not_cancelled() {
        let token = CancellationToken::default();
        assert!(!token.is_cancelled());
        token.cancel();
        assert!(token.is_cancelled());
    }
}
```

- [ ] **Step 5: Run focused tests**

Run:

```powershell
$env:CARGO_TARGET_DIR='C:\t\ha'; cargo test -p verbatim providers::types::tests:: --lib
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add src-tauri/src/lib.rs src-tauri/src/providers/mod.rs src-tauri/src/providers/types.rs
git commit -m "feat: add speech provider core types"
```

---

### Task 3: Add Capability Pair Semantics

**Files:**
- Modify: `src-tauri/src/providers/types.rs`

- [ ] **Step 1: Write failing capability tests**

Add to `src-tauri/src/providers/types.rs` tests:

```rust
#[test]
fn english_only_translation_supports_any_source_to_english_only() {
    let support = TranslationPairSupport::EnglishOnly;
    assert!(support.supports_pair("ar", "en"));
    assert!(support.supports_pair("fr", "en"));
    assert!(!support.supports_pair("en", "ar"));
    assert!(!support.supports_pair("fr", "es"));
}

#[test]
fn explicit_translation_pairs_require_exact_match() {
    let support = TranslationPairSupport::Explicit(vec![
        ("es".to_string(), "fr".to_string()),
        ("fr".to_string(), "yue".to_string()),
    ]);

    assert!(support.supports_pair("es", "fr"));
    assert!(support.supports_pair("fr", "yue"));
    assert!(!support.supports_pair("fr", "es"));
}

#[test]
fn capability_task_lookup_is_exact() {
    let capabilities = ProviderCapabilities {
        tasks: vec![SpeechTaskKind::Transcribe, SpeechTaskKind::TranslateText],
        translation_pairs: TranslationPairSupport::None,
        streaming: StreamingSupport::None,
        lifecycle: LifecycleCost::Cheap,
    };

    assert!(capabilities.supports_task(&SpeechTaskKind::Transcribe));
    assert!(capabilities.supports_task(&SpeechTaskKind::TranslateText));
    assert!(!capabilities.supports_task(&SpeechTaskKind::TranslateSpeech));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```powershell
$env:CARGO_TARGET_DIR='C:\t\ha'; cargo test -p verbatim providers::types::tests::english_only_translation_supports_any_source_to_english_only --lib
```

Expected: FAIL with missing `supports_pair`.

- [ ] **Step 3: Implement capability helper methods**

Add below the enum definitions in `src-tauri/src/providers/types.rs`:

```rust
impl TranslationPairSupport {
    pub fn supports_pair(&self, source_language: &str, target_language: &str) -> bool {
        match self {
            TranslationPairSupport::None => false,
            TranslationPairSupport::EnglishOnly => target_language == "en",
            TranslationPairSupport::Explicit(pairs) => pairs.iter().any(|(source, target)| {
                source == source_language && target == target_language
            }),
            TranslationPairSupport::AnyToAny { languages } => {
                languages.iter().any(|language| language == source_language)
                    && languages.iter().any(|language| language == target_language)
            }
        }
    }
}

impl ProviderCapabilities {
    pub fn supports_task(&self, task: &SpeechTaskKind) -> bool {
        self.tasks.iter().any(|candidate| candidate == task)
    }
}
```

- [ ] **Step 4: Run focused tests**

Run:

```powershell
$env:CARGO_TARGET_DIR='C:\t\ha'; cargo test -p verbatim providers::types::tests:: --lib
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/providers/types.rs
git commit -m "feat: add provider capability checks"
```

---

### Task 4: Bridge Current Models to ModelAsset

**Files:**
- Modify: `src-tauri/src/managers/model.rs`
- Modify: `src-tauri/src/providers/types.rs`

- [ ] **Step 1: Write failing locator tests**

Add to `src-tauri/src/providers/types.rs` tests:

```rust
fn minimal_model_info(id: &str, is_directory: bool) -> ModelInfo {
    ModelInfo {
        id: id.to_string(),
        name: "Test Model".to_string(),
        description: "Test".to_string(),
        filename: "test-model".to_string(),
        url: None,
        sha256: None,
        size_mb: 1,
        is_downloaded: true,
        is_downloading: false,
        partial_size: 0,
        is_directory,
        engine_type: crate::managers::model::EngineType::Whisper,
        accuracy_score: 0.0,
        speed_score: 0.0,
        supports_translation: false,
        is_recommended: false,
        supported_languages: vec![],
        supports_language_selection: true,
        is_custom: false,
    }
}

#[test]
fn model_asset_uses_directory_locator_for_directory_models() {
    let info = minimal_model_info("dir-model", true);
    let asset = ModelAsset::from_model_info(info, PathBuf::from("C:/models/dir-model"));
    assert!(matches!(asset.locator, ModelLocator::Directory(_)));
}

#[test]
fn model_asset_uses_file_locator_for_file_models() {
    let info = minimal_model_info("file-model", false);
    let asset = ModelAsset::from_model_info(info, PathBuf::from("C:/models/model.bin"));
    assert!(matches!(asset.locator, ModelLocator::File(_)));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```powershell
$env:CARGO_TARGET_DIR='C:\t\ha'; cargo test -p verbatim model_asset_uses_directory_locator_for_directory_models --lib
```

Expected: FAIL with missing `ModelAsset::from_model_info`.

- [ ] **Step 3: Implement the ModelAsset constructor**

Add to `src-tauri/src/providers/types.rs`:

```rust
impl ModelAsset {
    pub fn from_model_info(metadata: ModelInfo, resolved_path: PathBuf) -> Self {
        let locator = if metadata.is_directory {
            ModelLocator::Directory(resolved_path)
        } else {
            ModelLocator::File(resolved_path)
        };

        Self {
            id: metadata.id.clone(),
            locator,
            metadata,
        }
    }
}
```

- [ ] **Step 4: Add ModelManager asset resolver**

Add to `impl ModelManager` in `src-tauri/src/managers/model.rs`:

```rust
pub fn get_model_asset(&self, model_id: &str) -> Result<crate::providers::ModelAsset> {
    let model_info = self
        .get_model_info(model_id)
        .ok_or_else(|| anyhow::anyhow!("Model not found: {}", model_id))?;
    let model_path = self.get_model_path(model_id)?;
    Ok(crate::providers::ModelAsset::from_model_info(
        model_info,
        model_path,
    ))
}
```

- [ ] **Step 5: Run focused tests**

Run:

```powershell
$env:CARGO_TARGET_DIR='C:\t\ha'; cargo test -p verbatim providers::types::tests::model_asset --lib
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add src-tauri/src/providers/types.rs src-tauri/src/managers/model.rs
git commit -m "feat: map model metadata to provider assets"
```

---

### Task 5: Add Provider Registry With Fake Provider Tests

**Files:**
- Create: `src-tauri/src/providers/registry.rs`
- Modify: `src-tauri/src/providers/mod.rs`

- [ ] **Step 1: Write failing registry tests**

Create `src-tauri/src/providers/registry.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{
        LifecycleCost, ProviderCapabilities, SpeechTaskKind, StreamingSupport,
        TranslationPairSupport,
    };

    struct FakeProvider {
        id: &'static str,
        capabilities: ProviderCapabilities,
    }

    impl ProviderDescriptor for FakeProvider {
        fn provider_id(&self) -> &'static str {
            self.id
        }

        fn capabilities(&self, _asset: &crate::providers::ModelAsset) -> ProviderCapabilities {
            self.capabilities.clone()
        }
    }

    #[test]
    fn registry_selects_provider_supporting_task() {
        let asset = test_asset();
        let registry = ProviderRegistry::new(vec![
            Box::new(FakeProvider {
                id: "nope",
                capabilities: ProviderCapabilities {
                    tasks: vec![SpeechTaskKind::TranslateText],
                    translation_pairs: TranslationPairSupport::None,
                    streaming: StreamingSupport::None,
                    lifecycle: LifecycleCost::NoLoad,
                },
            }),
            Box::new(FakeProvider {
                id: "asr",
                capabilities: ProviderCapabilities {
                    tasks: vec![SpeechTaskKind::Transcribe],
                    translation_pairs: TranslationPairSupport::None,
                    streaming: StreamingSupport::None,
                    lifecycle: LifecycleCost::Cheap,
                },
            }),
        ]);

        let selected = registry
            .select(&asset, &SpeechTaskKind::Transcribe)
            .expect("provider should be selected");
        assert_eq!(selected.provider_id(), "asr");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```powershell
$env:CARGO_TARGET_DIR='C:\t\ha'; cargo test -p verbatim providers::registry::tests:: --lib
```

Expected: FAIL with missing `ProviderRegistry` and `ProviderDescriptor`.

- [ ] **Step 3: Implement registry and test helper**

Replace `src-tauri/src/providers/registry.rs` with:

```rust
use crate::providers::{ModelAsset, ProviderCapabilities, SpeechTaskKind};

pub trait ProviderDescriptor: Send + Sync {
    fn provider_id(&self) -> &'static str;
    fn capabilities(&self, asset: &ModelAsset) -> ProviderCapabilities;
}

pub struct ProviderRegistry {
    providers: Vec<Box<dyn ProviderDescriptor>>,
}

impl ProviderRegistry {
    pub fn new(providers: Vec<Box<dyn ProviderDescriptor>>) -> Self {
        Self { providers }
    }

    pub fn select(
        &self,
        asset: &ModelAsset,
        task: &SpeechTaskKind,
    ) -> Option<&dyn ProviderDescriptor> {
        self.providers
            .iter()
            .map(|provider| provider.as_ref())
            .find(|provider| provider.capabilities(asset).supports_task(task))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managers::model::{EngineType, ModelInfo};
    use crate::providers::{
        LifecycleCost, ModelAsset, ProviderCapabilities, SpeechTaskKind, StreamingSupport,
        TranslationPairSupport,
    };
    use std::path::PathBuf;

    fn test_asset() -> ModelAsset {
        ModelAsset::from_model_info(
            ModelInfo {
                id: "test".to_string(),
                name: "Test".to_string(),
                description: "Test".to_string(),
                filename: "test.bin".to_string(),
                url: None,
                sha256: None,
                size_mb: 1,
                is_downloaded: true,
                is_downloading: false,
                partial_size: 0,
                is_directory: false,
                engine_type: EngineType::Whisper,
                accuracy_score: 0.0,
                speed_score: 0.0,
                supports_translation: false,
                is_recommended: false,
                supported_languages: vec![],
                supports_language_selection: true,
                is_custom: false,
            },
            PathBuf::from("test.bin"),
        )
    }

    struct FakeProvider {
        id: &'static str,
        capabilities: ProviderCapabilities,
    }

    impl ProviderDescriptor for FakeProvider {
        fn provider_id(&self) -> &'static str {
            self.id
        }

        fn capabilities(&self, _asset: &ModelAsset) -> ProviderCapabilities {
            self.capabilities.clone()
        }
    }

    #[test]
    fn registry_selects_provider_supporting_task() {
        let asset = test_asset();
        let registry = ProviderRegistry::new(vec![
            Box::new(FakeProvider {
                id: "nope",
                capabilities: ProviderCapabilities {
                    tasks: vec![SpeechTaskKind::TranslateText],
                    translation_pairs: TranslationPairSupport::None,
                    streaming: StreamingSupport::None,
                    lifecycle: LifecycleCost::NoLoad,
                },
            }),
            Box::new(FakeProvider {
                id: "asr",
                capabilities: ProviderCapabilities {
                    tasks: vec![SpeechTaskKind::Transcribe],
                    translation_pairs: TranslationPairSupport::None,
                    streaming: StreamingSupport::None,
                    lifecycle: LifecycleCost::Cheap,
                },
            }),
        ]);

        let selected = registry
            .select(&asset, &SpeechTaskKind::Transcribe)
            .expect("provider should be selected");
        assert_eq!(selected.provider_id(), "asr");
    }
}
```

Update `src-tauri/src/providers/mod.rs`:

```rust
pub mod registry;
pub mod types;

pub use registry::*;
pub use types::*;
```

- [ ] **Step 4: Run focused tests**

Run:

```powershell
$env:CARGO_TARGET_DIR='C:\t\ha'; cargo test -p verbatim providers::registry::tests:: --lib
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/providers/mod.rs src-tauri/src/providers/registry.rs
git commit -m "feat: add provider registry selection"
```

---

### Task 6: Move Current transcribe-rs Engines Behind Provider Adapter

**Files:**
- Create: `src-tauri/src/providers/transcribe_rs.rs`
- Modify: `src-tauri/src/providers/mod.rs`
- Modify: `src-tauri/src/managers/transcription.rs`

- [ ] **Step 1: Write adapter capability tests**

Create `src-tauri/src/providers/transcribe_rs.rs` with tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::managers::model::{EngineType, ModelInfo};
    use crate::providers::{ModelAsset, SpeechTaskKind, TranslationPairSupport};
    use std::path::PathBuf;

    fn asset(engine_type: EngineType, supports_translation: bool) -> ModelAsset {
        ModelAsset::from_model_info(
            ModelInfo {
                id: "test".to_string(),
                name: "Test".to_string(),
                description: "Test".to_string(),
                filename: "test.bin".to_string(),
                url: None,
                sha256: None,
                size_mb: 1,
                is_downloaded: true,
                is_downloading: false,
                partial_size: 0,
                is_directory: false,
                engine_type,
                accuracy_score: 0.0,
                speed_score: 0.0,
                supports_translation,
                is_recommended: false,
                supported_languages: vec!["en".to_string(), "ar".to_string()],
                supports_language_selection: true,
                is_custom: false,
            },
            PathBuf::from("test.bin"),
        )
    }

    #[test]
    fn transcribe_rs_provider_supports_transcription_for_current_engines() {
        let provider = TranscribeRsProvider::new();
        let capabilities = provider.capabilities(&asset(EngineType::Whisper, false));
        assert!(capabilities.supports_task(&SpeechTaskKind::Transcribe));
    }

    #[test]
    fn transcribe_rs_provider_reports_english_only_translation_when_model_supports_translation() {
        let provider = TranscribeRsProvider::new();
        let capabilities = provider.capabilities(&asset(EngineType::Whisper, true));
        assert_eq!(capabilities.translation_pairs, TranslationPairSupport::EnglishOnly);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```powershell
$env:CARGO_TARGET_DIR='C:\t\ha'; cargo test -p verbatim providers::transcribe_rs::tests:: --lib
```

Expected: FAIL with missing `TranscribeRsProvider`.

- [ ] **Step 3: Move the current engine enum and load match**

Create `src-tauri/src/providers/transcribe_rs.rs` with:

```rust
use crate::managers::model::EngineType;
use crate::providers::{
    EngineProvider, LifecycleCost, ModelAsset, ModelLocator, ProviderCapabilities, SpeechInput,
    SpeechRequest, SpeechResponse, SpeechTaskKind, StreamingSupport, TranslationPairSupport,
};
use anyhow::Result;
use transcribe_rs::{
    onnx::{
        canary::CanaryModel,
        cohere::CohereModel,
        gigaam::GigaAMModel,
        moonshine::{MoonshineModel, MoonshineVariant, StreamingModel},
        parakeet::ParakeetModel,
        sense_voice::SenseVoiceModel,
        Quantization,
    },
    whisper_cpp::WhisperEngine,
};

pub enum LoadedEngine {
    Whisper(WhisperEngine),
    Parakeet(ParakeetModel),
    Moonshine(MoonshineModel),
    MoonshineStreaming(StreamingModel),
    SenseVoice(SenseVoiceModel),
    GigaAM(GigaAMModel),
    Canary(CanaryModel),
    Cohere(CohereModel),
}

pub struct TranscribeRsProvider {
    engine: Option<LoadedEngine>,
    model_id: Option<String>,
}

impl TranscribeRsProvider {
    pub fn new() -> Self {
        Self {
            engine: None,
            model_id: None,
        }
    }
}

impl Default for TranscribeRsProvider {
    fn default() -> Self {
        Self::new()
    }
}
```

Then move the current `LoadedEngine` enum out of `src-tauri/src/managers/transcription.rs` and import it from the provider module while compilation is restored.

- [ ] **Step 4: Implement provider capabilities**

Add to `impl EngineProvider for TranscribeRsProvider`:

```rust
fn provider_id(&self) -> &'static str {
    "transcribe_rs"
}

fn capabilities(&self, asset: &ModelAsset) -> ProviderCapabilities {
    ProviderCapabilities {
        tasks: vec![SpeechTaskKind::Transcribe],
        translation_pairs: if asset.metadata.supports_translation {
            TranslationPairSupport::EnglishOnly
        } else {
            TranslationPairSupport::None
        },
        streaming: match asset.metadata.engine_type {
            EngineType::MoonshineStreaming => StreamingSupport::PartialText,
            _ => StreamingSupport::None,
        },
        lifecycle: LifecycleCost::Expensive,
    }
}
```

- [ ] **Step 5: Implement load with ModelLocator**

Add to `impl EngineProvider for TranscribeRsProvider`:

```rust
fn load(&mut self, asset: &ModelAsset) -> Result<()> {
    let model_path = match &asset.locator {
        ModelLocator::File(path) | ModelLocator::Directory(path) => path,
        ModelLocator::ManagedServer { .. } | ModelLocator::ExternalHttp { .. } => {
            return Err(anyhow::anyhow!(
                "transcribe_rs provider requires a local file or directory model asset"
            ));
        }
    };

    let loaded = match asset.metadata.engine_type {
        EngineType::Whisper => LoadedEngine::Whisper(WhisperEngine::load(model_path)?),
        EngineType::Parakeet => {
            LoadedEngine::Parakeet(ParakeetModel::load(model_path, &Quantization::Int8)?)
        }
        EngineType::Moonshine => LoadedEngine::Moonshine(MoonshineModel::load(
            model_path,
            MoonshineVariant::Base,
            &Quantization::default(),
        )?),
        EngineType::MoonshineStreaming => LoadedEngine::MoonshineStreaming(
            StreamingModel::load(model_path, 0, &Quantization::default())?,
        ),
        EngineType::SenseVoice => {
            LoadedEngine::SenseVoice(SenseVoiceModel::load(model_path, &Quantization::Int8)?)
        }
        EngineType::GigaAM => {
            LoadedEngine::GigaAM(GigaAMModel::load(model_path, &Quantization::Int8)?)
        }
        EngineType::Canary => {
            LoadedEngine::Canary(CanaryModel::load(model_path, &Quantization::Int8)?)
        }
        EngineType::Cohere => {
            LoadedEngine::Cohere(CohereModel::load(model_path, &Quantization::Int8)?)
        }
    };

    self.engine = Some(loaded);
    self.model_id = Some(asset.id.clone());
    Ok(())
}

fn unload(&mut self) {
    self.engine = None;
    self.model_id = None;
}
```

- [ ] **Step 6: Run capability tests**

Run:

```powershell
$env:CARGO_TARGET_DIR='C:\t\ha'; cargo test -p verbatim providers::transcribe_rs::tests:: --lib
```

Expected: PASS.

- [ ] **Step 7: Commit**

```powershell
git add src-tauri/src/providers/mod.rs src-tauri/src/providers/transcribe_rs.rs src-tauri/src/managers/transcription.rs
git commit -m "feat: add transcribe-rs provider adapter"
```

---

### Task 7: Move Inference Dispatch Behind Provider Run

**Files:**
- Modify: `src-tauri/src/providers/transcribe_rs.rs`
- Modify: `src-tauri/src/managers/transcription.rs`

- [ ] **Step 1: Write request-construction tests**

Add to `src-tauri/src/managers/transcription.rs` tests:

```rust
#[test]
fn speech_request_translation_is_absent_when_legacy_toggle_is_off() {
    let request = build_transcription_request(
        vec![0.0, 1.0],
        "auto",
        false,
        &["en".to_string(), "ar".to_string()],
        &[],
    );

    assert!(request.translation.is_none());
}

#[test]
fn speech_request_uses_english_target_for_legacy_translation() {
    let request = build_transcription_request(
        vec![0.0, 1.0],
        "auto",
        true,
        &["en".to_string(), "ar".to_string()],
        &[],
    );

    let translation = request.translation.expect("translation should be present");
    assert_eq!(translation.target_language, "en");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```powershell
$env:CARGO_TARGET_DIR='C:\t\ha'; cargo test -p verbatim speech_request_translation --lib
```

Expected: FAIL with missing `build_transcription_request`.

- [ ] **Step 3: Add request construction helper**

Add to `src-tauri/src/managers/transcription.rs`:

```rust
fn build_transcription_request(
    audio: Vec<f32>,
    selected_language: &str,
    translate_to_english: bool,
    language_shortlist: &[String],
    custom_words: &[String],
) -> crate::providers::SpeechRequest {
    let source_language = if selected_language == "auto" {
        crate::providers::LanguageSelection::Auto
    } else {
        crate::providers::LanguageSelection::Language(selected_language.to_string())
    };

    crate::providers::SpeechRequest {
        task: crate::providers::SpeechTaskKind::Transcribe,
        input: crate::providers::SpeechInput::Audio(std::sync::Arc::from(audio)),
        translation: translate_to_english.then_some(crate::providers::TranslationTarget {
            source_language,
            target_language: "en".to_string(),
        }),
        language_shortlist: language_shortlist.to_vec(),
        custom_words: custom_words.to_vec(),
        cancellation: crate::providers::CancellationToken::default(),
    }
}
```

- [ ] **Step 4: Move engine dispatch into `TranscribeRsProvider::run`**

Move the current `match &mut engine` from `TranscriptionManager::transcribe` into `src-tauri/src/providers/transcribe_rs.rs`.

Implement this function shape:

```rust
fn run(&mut self, request: SpeechRequest) -> Result<SpeechResponse> {
    let audio = match request.input {
        SpeechInput::Audio(audio) => audio,
        SpeechInput::Text(_) => {
            return Err(anyhow::anyhow!(
                "transcribe_rs provider requires audio input for transcription"
            ));
        }
    };

    if request.cancellation.is_cancelled() {
        return Err(anyhow::anyhow!("transcription cancelled before provider run"));
    }

    let model_id = self
        .model_id
        .clone()
        .ok_or_else(|| anyhow::anyhow!("provider has no loaded model"))?;

    let translated = request.translation.is_some();

    let result = match self
        .engine
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("provider engine is not loaded"))?
    {
        LoadedEngine::Whisper(whisper_engine) => {
            // Reuse existing Whisper parameter construction.
            // Move the existing code here, preserving language and prompt behavior.
            run_whisper_engine(whisper_engine, &audio, &request)?
        }
        LoadedEngine::Parakeet(parakeet_engine) => run_parakeet_engine(parakeet_engine, &audio)?,
        LoadedEngine::Moonshine(moonshine_engine) => run_speech_model(moonshine_engine, &audio)?,
        LoadedEngine::MoonshineStreaming(streaming_engine) => {
            run_speech_model(streaming_engine, &audio)?
        }
        LoadedEngine::SenseVoice(sense_voice_engine) => {
            run_sense_voice_engine(sense_voice_engine, &audio, &request)?
        }
        LoadedEngine::GigaAM(gigaam_engine) => run_speech_model(gigaam_engine, &audio)?,
        LoadedEngine::Canary(canary_engine) => run_canary_engine(canary_engine, &audio, &request)?,
        LoadedEngine::Cohere(cohere_engine) => run_cohere_engine(cohere_engine, &audio, &request)?,
    };

    Ok(SpeechResponse {
        text: result.text,
        detected_language: None,
        translated,
        provider_id: self.provider_id(),
        model_id,
    })
}
```

Do not change language behavior in this task. Move existing logic exactly, then compile.

- [ ] **Step 5: Keep panic boundary in orchestrator**

In `TranscriptionManager::transcribe`, keep `catch_unwind(AssertUnwindSafe(...))` around the provider `run()` call. On panic, call provider `unload()`, clear `current_model_id`, and emit the existing `model-state-changed` unloaded event.

- [ ] **Step 6: Run full Rust library tests**

Run:

```powershell
$env:CARGO_TARGET_DIR='C:\t\ha'; cargo test -p verbatim --lib
```

Expected: PASS.

- [ ] **Step 7: Commit**

```powershell
git add src-tauri/src/providers/transcribe_rs.rs src-tauri/src/managers/transcription.rs
git commit -m "refactor: route transcription through provider adapter"
```

---

### Task 8: Add General Translation Settings and Legacy Migration

**Files:**
- Modify: `src-tauri/src/settings.rs`
- Modify: `src-tauri/src/shortcut/mod.rs`
- Modify: `src/stores/settingsStore.ts`

- [ ] **Step 1: Write settings migration tests**

Add to `src-tauri/src/settings.rs` tests:

```rust
#[test]
fn legacy_translate_to_english_maps_to_translation_request() {
    let mut settings = get_default_settings();
    settings.translate_to_english = true;

    let changed = ensure_translation_defaults(&mut settings);

    assert!(changed);
    assert!(settings.translation_enabled);
    assert_eq!(
        settings
            .translation_request
            .as_ref()
            .expect("translation request")
            .target_language,
        "en"
    );
}

#[test]
fn translation_request_absent_when_translation_is_disabled() {
    let settings = get_default_settings();
    assert!(!settings.translation_enabled);
    assert!(settings.translation_request.is_none());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```powershell
$env:CARGO_TARGET_DIR='C:\t\ha'; cargo test -p verbatim legacy_translate_to_english_maps_to_translation_request --lib
```

Expected: FAIL with missing fields/function.

- [ ] **Step 3: Add translation settings types**

Add to `src-tauri/src/settings.rs`:

```rust
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum TranslationRoute {
    Auto,
    DirectSpeech,
    TextAfterTranscription,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Type)]
pub struct TranslationRequestSettings {
    pub source_language: String,
    pub target_language: String,
    pub route: TranslationRoute,
}
```

Add fields to `AppSettings`:

```rust
#[serde(default)]
pub translation_enabled: bool,
#[serde(default)]
pub translation_request: Option<TranslationRequestSettings>,
#[serde(default)]
pub translation_provider_id: Option<String>,
#[serde(default)]
pub translation_model_id: Option<String>,
```

Add defaults in `get_default_settings()`:

```rust
translation_enabled: false,
translation_request: None,
translation_provider_id: None,
translation_model_id: None,
```

- [ ] **Step 4: Add migration helper**

Add near other ensure helpers:

```rust
fn ensure_translation_defaults(settings: &mut AppSettings) -> bool {
    if settings.translate_to_english && settings.translation_request.is_none() {
        settings.translation_enabled = true;
        settings.translation_request = Some(TranslationRequestSettings {
            source_language: "auto".to_string(),
            target_language: "en".to_string(),
            route: TranslationRoute::Auto,
        });
        return true;
    }

    if !settings.translation_enabled && settings.translation_request.is_some() {
        settings.translation_request = None;
        return true;
    }

    false
}
```

Call it anywhere `ensure_post_process_defaults` and `ensure_adaptive_defaults` are called:

```rust
let translation_changed = ensure_translation_defaults(&mut settings);
if post_process_changed || adaptive_changed || translation_changed {
    write_settings(app, &settings);
}
```

- [ ] **Step 5: Add command path update**

In `src-tauri/src/shortcut/mod.rs`, replace the implementation of the command that currently writes `translate_to_english` with logic that writes both legacy and new settings:

```rust
settings.translate_to_english = enabled;
settings.translation_enabled = enabled;
settings.translation_request = enabled.then_some(crate::settings::TranslationRequestSettings {
    source_language: "auto".to_string(),
    target_language: "en".to_string(),
    route: crate::settings::TranslationRoute::Auto,
});
```

This preserves compatibility until the UI is fully moved.

- [ ] **Step 6: Run settings tests**

Run:

```powershell
$env:CARGO_TARGET_DIR='C:\t\ha'; cargo test -p verbatim settings::tests:: --lib
```

Expected: PASS.

- [ ] **Step 7: Regenerate bindings if the repo command requires it**

Run the normal build:

```powershell
bun run build
```

Expected: PASS and `src/bindings.ts` updated if specta generation runs during build.

- [ ] **Step 8: Commit**

```powershell
git add src-tauri/src/settings.rs src-tauri/src/shortcut/mod.rs src/stores/settingsStore.ts src/bindings.ts
git commit -m "feat: add general translation settings"
```

---

### Task 9: Replace English-Only Translation UI

**Files:**
- Modify: `src/components/settings/TranslateToEnglish.tsx`
- Modify: `src/components/settings/general/ModelSettingsCard.tsx`
- Modify: `src/stores/settingsStore.ts`
- Modify: `src/i18n/locales/en/translation.json`

- [ ] **Step 1: Write UI state expectation as component logic test or pure helper test**

If the project does not have component tests for settings, add a pure helper beside `TranslateToEnglish.tsx`:

Create `src/components/settings/translationOptions.ts`:

```ts
export type TranslationSupport =
  | { kind: "none" }
  | { kind: "english_only" }
  | { kind: "multi"; languages: string[] };

export function canUseTargetLanguage(
  support: TranslationSupport,
  targetLanguage: string,
): boolean {
  if (support.kind === "none") return false;
  if (support.kind === "english_only") return targetLanguage === "en";
  return support.languages.includes(targetLanguage);
}
```

Create `src/components/settings/translationOptions.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { canUseTargetLanguage } from "./translationOptions";

describe("canUseTargetLanguage", () => {
  it("allows only English for english-only native translation", () => {
    expect(canUseTargetLanguage({ kind: "english_only" }, "en")).toBe(true);
    expect(canUseTargetLanguage({ kind: "english_only" }, "fr")).toBe(false);
  });

  it("allows configured multilingual targets", () => {
    expect(
      canUseTargetLanguage({ kind: "multi", languages: ["en", "fr", "ar"] }, "fr"),
    ).toBe(true);
  });
});
```

- [ ] **Step 2: Run frontend tests or typecheck**

If Vitest is configured, run:

```powershell
bunx vitest run src/components/settings/translationOptions.test.ts
```

If Vitest is not configured, run:

```powershell
bun run build
```

Expected: helper compiles and tests pass when test runner exists.

- [ ] **Step 3: Rename visible UI semantics**

In `src/components/settings/TranslateToEnglish.tsx`, change the component from an English-only toggle to a translation control that:

- Shows `Translation` as the label.
- Defaults to off.
- Shows target language dropdown when enabled.
- Keeps English as the only enabled native target for legacy `supports_translation` models until provider capabilities are exposed to the frontend.

Use existing dropdown/toggle components already used in settings. Do not add a new UI library.

- [ ] **Step 4: Update English source strings**

In `src/i18n/locales/en/translation.json`, replace the existing `translateToEnglish` block with keys equivalent to:

```json
"translation": {
  "label": "Translation",
  "description": "Translate dictated output only when this is enabled",
  "descriptionUnsupported": "{{model}} does not support native translation. Text translation requires a translation provider.",
  "targetLanguage": "Target language",
  "off": "Off"
}
```

Keep the old key temporarily if other locales/components still reference it.

- [ ] **Step 5: Run lint and build**

Run:

```powershell
bun run lint
bun run build
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add src/components/settings/TranslateToEnglish.tsx src/components/settings/translationOptions.ts src/components/settings/translationOptions.test.ts src/components/settings/general/ModelSettingsCard.tsx src/stores/settingsStore.ts src/i18n/locales/en/translation.json
git commit -m "feat: generalize translation settings UI"
```

---

### Task 10: Final Verification and Packaging

**Files:**
- No planned source edits.
- Generated installer files are outside the repo under `C:\t\ha\release\bundle` and copied to `C:\Users\Admin\Downloads`.

- [ ] **Step 1: Run Rust tests**

Run:

```powershell
$env:CARGO_TARGET_DIR='C:\t\ha'; cargo test -p verbatim --lib
```

Expected: PASS. Record exact passed count.

- [ ] **Step 2: Run frontend lint**

Run:

```powershell
bun run lint
```

Expected: PASS.

- [ ] **Step 3: Run frontend build**

Run:

```powershell
bun run build
```

Expected: PASS. Existing chunk-size warning is acceptable.

- [ ] **Step 4: Build Windows installers**

Run:

```powershell
$env:CARGO_TARGET_DIR='C:\t\ha'; bun run tauri build
```

Expected:

```text
C:\t\ha\release\bundle\msi\Verbatim_0.8.3_x64_en-US.msi
C:\t\ha\release\bundle\nsis\Verbatim_0.8.3_x64-setup.exe
```

- [ ] **Step 5: Copy installers to Downloads and hash**

Run:

```powershell
Copy-Item -LiteralPath 'C:\t\ha\release\bundle\nsis\Verbatim_0.8.3_x64-setup.exe' -Destination 'C:\Users\Admin\Downloads\Verbatim_0.8.3_x64-setup.exe' -Force
Copy-Item -LiteralPath 'C:\t\ha\release\bundle\msi\Verbatim_0.8.3_x64_en-US.msi' -Destination 'C:\Users\Admin\Downloads\Verbatim_0.8.3_x64_en-US.msi' -Force
Get-FileHash -Algorithm SHA256 'C:\Users\Admin\Downloads\Verbatim_0.8.3_x64-setup.exe','C:\Users\Admin\Downloads\Verbatim_0.8.3_x64_en-US.msi' | Select-Object Path,Hash | ConvertTo-Json
```

Expected: two SHA256 hashes returned.

- [ ] **Step 6: Confirm no source files changed during packaging**

Run:

```powershell
git status --short
```

Expected: no output. If files changed, stop and inspect them with `git diff`; do not commit packaging artifacts.

---

## Self-Review

- Spec coverage:
  - Capability query with language-pair support: Task 3.
  - `ModelAsset` and locator instead of raw path: Task 4.
  - `SpeechInput` sum type and `Arc<[f32]>`: Task 2.
  - Lifecycle policy centralized in orchestrator: Task 7.
  - Characterization tests before refactor: Task 1 and Task 5.
  - Streaming not ossified into batch interface: Task 2 and ADR retained.
  - Cancellation token included: Task 2.
  - Translation settings generalized: Task 8 and Task 9.
  - Legacy migration path: Task 8.

- Plan scan:
  - No open-ended markers or unspecified test commands remain.

- Type consistency:
  - `SpeechInput`, `SpeechTaskKind`, `TranslationTarget`, `SpeechRequest`, `SpeechResponse`, `ModelAsset`, `ModelLocator`, `ProviderCapabilities`, `TranslationPairSupport`, `StreamingSupport`, `LifecycleCost`, `CancellationToken`, and `EngineProvider` names match across tasks.

## Execution Recommendation

Use subagent-driven execution for Tasks 1 through 9, with one fresh implementer per task and review between tasks. Task 10 should run inline on the local Windows machine because it packages the app and writes installers to Downloads.
