# ADR-0001: Engine Provider Architecture

## Status
Proposed

## Context
Verbatim currently exposes models as a flat catalog through `ModelInfo`, with an
`EngineType` enum describing the runtime needed for each model. `ModelManager`
owns model metadata, downloads, and local file discovery. `TranscriptionManager`
owns model loading, active engine lifetime, transcription execution, translation
flags, language hints, custom-word handling, filtering, logging, and panic
recovery.

That structure works for bundled local engines supported by `transcribe-rs`, but
it becomes too tightly coupled as soon as Verbatim adds providers such as
Qwen3-ASR, SeamlessM4T, Granite Speech, Voxtral, local Python runtimes, local
OpenAI-compatible endpoints, or cloud/self-hosted APIs. Those providers differ
in capabilities:

- Some transcribe only.
- Some translate only to English.
- Some translate to many target languages.
- Some stream partial results.
- Some need local files, some need directories, some need a server process.
- Some are local/offline, some are networked, some are experimental.

The current `translate_to_english: bool` setting is also too narrow. Translation
must become an explicit workflow with source and target languages instead of an
adaptive side effect or an English-only toggle.

## Decision
Introduce an engine/provider architecture with three separated concepts:

1. `ModelAsset`
   - Describes something the app can download, verify, delete, and store.
   - Examples: `ggml-large-v3-turbo.bin`, `parakeet-v3-int8/`,
     `Qwen3-ASR-0.6B/`, `seamless-m4t-v2-large/`.
   - Owned by model/package management.

2. `EngineProvider`
   - Describes a runtime capable of executing one or more tasks.
   - Examples: `whisper_cpp`, `transcribe_rs_onnx`, `python_transformers`,
     `vllm_http`, `openai_compatible_http`.
   - Owns loading, readiness checks, invocation, capability reporting, and
     provider-specific diagnostics.

3. `SpeechTask`
   - Describes what the user asked the system to do.
   - Initial tasks:
     - `Transcribe`
     - `TranslateSpeech`
     - `TranslateText`
     - `PostProcessText`
   - Each task carries source language, target language, language shortlist,
     prompt/context, and output constraints.

`TranscriptionManager` should become an orchestrator:

- Resolve the selected user intent into a `SpeechTask`.
- Ask the registry for a provider that supports the task and selected model.
- Execute the provider.
- Run shared post-output guards: no unrequested translation, custom words,
  filler filtering, adaptive formatting, history persistence.

Provider-specific logic should move behind an interface rather than continuing
to grow inside a single `match LoadedEngine`.

## Target Interfaces
The first implementation should keep the interface small:

```rust
pub enum SpeechTaskKind {
    Transcribe,
    TranslateSpeech,
    TranslateText,
    PostProcessText,
}

pub struct SpeechRequest {
    pub task: SpeechTaskKind,
    pub audio: Option<Vec<f32>>,
    pub text: Option<String>,
    pub source_language: LanguageSelection,
    pub target_language: Option<String>,
    pub language_shortlist: Vec<String>,
    pub custom_words: Vec<String>,
}

pub struct SpeechResponse {
    pub text: String,
    pub detected_language: Option<String>,
    pub translated: bool,
    pub provider_id: String,
    pub model_id: String,
}

pub trait EngineProvider: Send {
    fn provider_id(&self) -> &'static str;
    fn supports(&self, model: &ModelInfo, task: &SpeechTaskKind) -> bool;
    fn load(&mut self, model: &ModelInfo, model_path: &Path) -> anyhow::Result<()>;
    fn unload(&mut self);
    fn run(&mut self, request: SpeechRequest) -> anyhow::Result<SpeechResponse>;
}
```

This does not need to be the final interface. It is the smallest useful seam for
moving existing engine branching out of `TranscriptionManager`.

## Translation Model
Replace the English-only setting over time:

Current:

```text
translate_to_english: bool
```

Target:

```text
translation_enabled: bool
translation_source_language: "auto" | language_code
translation_target_language: language_code | null
translation_mode: "native" | "text" | "speech" | "auto"
translation_provider_id: string | null
translation_model_id: string | null
```

Rules:

- Translation is off by default.
- Dictation never translates unless translation is explicitly enabled.
- Adaptive profiles may format or clean text but must not translate.
- Native ASR translation is allowed only when the selected provider reports that
  exact source/target pair as supported.
- If native translation is not supported, Verbatim may offer text translation as
  a second step through a translation-capable provider.

## Migration Plan
Implement this in small slices:

1. Add provider/task types without changing behavior.
2. Wrap the current `transcribe-rs` engines in one provider adapter.
3. Move the existing `LoadedEngine` match behind that provider.
4. Keep current model IDs and settings working.
5. Add translation settings while preserving `translate_to_english` as a legacy
   migration input.
6. Add a provider registry and capability filtering for UI.
7. Add experimental providers:
   - Qwen3-ASR for multilingual dictation.
   - SeamlessM4T for broad private translation experiments.
   - Granite Speech for Apache-licensed multilingual translation where covered.
8. Only after the provider seam is stable, add streaming providers such as
   Voxtral or vLLM-backed Qwen.

## Alternatives Considered

### Keep adding variants to `EngineType`
This is the smallest immediate change, but it keeps growing the large
`TranscriptionManager` match and forces every provider to fit the same local
model lifecycle. It will make translation and streaming awkward.

### Use only OpenAI-compatible HTTP providers
This would simplify runtime management, but it would weaken Verbatim's local and
offline identity. It also does not handle local Whisper/ONNX models cleanly.

### Add separate managers for ASR, translation, and post-processing
This may become useful later, but it is too much structure before the task and
provider capabilities are explicit. Start with the provider seam first.

## Consequences

Benefits:

- New providers can be added without editing the transcription orchestration
  each time.
- Translation can support arbitrary target languages instead of English only.
- UI can filter models by task capability instead of hard-coded flags.
- Local, server-backed, and cloud providers can coexist.
- Tests can exercise provider selection and output safety without loading real
  model weights.

Trade-offs:

- More types and migration work up front.
- Existing `ModelInfo` will need to evolve into task/provider capability data.
- Some providers will require sidecar runtimes, health checks, and clearer
  installation UX.

Risks:

- A too-general interface could become abstract noise. Keep the first seam only
  as wide as current behavior plus translation requires.
- Python/vLLM providers may complicate Windows packaging. Treat them as
  experimental until installation and process lifecycle are reliable.
- Non-commercial licenses, such as SeamlessM4T's CC-BY-NC-4.0, must be clearly
  surfaced and not silently mixed into a public/commercial release.

## Verification
The provider architecture is working when:

- Existing Whisper, Parakeet, Moonshine, SenseVoice, GigaAM, Canary, and Cohere
  behavior passes current tests unchanged.
- Translation cannot occur unless `translation_enabled` is true.
- UI can distinguish dictation models from translation providers.
- Adding a new provider requires adding an adapter and metadata, not editing the
  main transcription orchestration.
- A fake provider can be tested without downloading model weights.
