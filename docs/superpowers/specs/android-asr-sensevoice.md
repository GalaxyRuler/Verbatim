# Android SenseVoice ASR Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Stage S1 is a proof spike and must stop at the findings checkpoint. Do not begin S2 until the maintainer has reviewed S1 device/AVD evidence.

**Goal:** Add a SenseVoice multilingual offline Android ASR engine for zh/en/ja/ko/yue, first proving that it loads and transcribes on Android, then integrating it as a no-streaming model pack without regressing existing zipformer+whisper packs.

**Architecture:** Keep the current zipformer+whisper path intact and add SenseVoice as a second offline engine behind an explicit engine-kind switch. S1 adds only enough Rust recognizer and smoke-harness code to prove `OfflineSenseVoiceModelConfig` on real Android inputs. S2 generalizes pack metadata and runtime session flow so packs with no streaming tier run VAD-segmented offline-only transcription, emit no partials, and let the bubble show recording/transcribing/final states.

**Tech Stack:** Rust `sherpa-onnx` 1.13.3, Tauri Android app crate, JNI bridge, Kotlin `FloatingBubbleService`, Silero VAD, Hugging Face pinned model assets, Bun/TypeScript i18n checks, Gradle unit tests, Maestro Android E2E, Android NDK r28 16 KB alignment guard.

---

## Current Repo Facts

- Current checkout at plan time was `codex/android-tier-a-models`; create the requested implementation branches from the intended integration base before coding.
- `docs/superpowers/` is ignored by `.gitignore`, so this file must be staged with `git add -f docs/superpowers/specs/android-asr-sensevoice.md`.
- The app already depends on `sherpa-onnx = "1.13"` for Android/iOS and the lockfile resolves `sherpa-onnx 1.13.3`.
- `src-tauri/src/asr/offline.rs` currently builds only `OfflineWhisperModelConfig`.
- `src-tauri/src/commands/asr.rs` currently constructs both `StreamingRecognizer` and `OfflineRecognizer` unconditionally.
- `src-tauri/src/asr/mod.rs::AsrModelPaths` currently assumes this required layout:

```text
streaming/encoder.onnx
streaming/decoder.onnx
streaming/joiner.onnx
streaming/tokens.txt
whisper/encoder.onnx
whisper/decoder.onnx
whisper/tokens.txt
silero_vad_v4.onnx
```

- `src-tauri/gen/android/app/src/main/java/com/galaxyruler/verbatim/FloatingBubbleService.kt` hardcodes the same required ASR files in `REQUIRED_ENGINE_MODEL_FILES`.
- Android model pack cards already localize pack display text via `android.models.packs.<pack-id>.displayName` and `.description`, with Rust-provided strings as defaults.
- Locale directories are `ar`, `bg`, `cs`, `de`, `en`, `es`, `fr`, `he`, `it`, `ja`, `ko`, `pl`, `pt`, `ru`, `sv`, `tr`, `uk`, `vi`, `zh`, and `zh-TW`.

## Pinned SenseVoice Assets

Source repository:

```text
https://huggingface.co/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17
```

Immutable revision:

```text
2365baeacb507f821a0c8120fcee3d484dba7a07
```

Model pack id:

```text
sensevoice-multilingual-zh-en-ja-ko-yue
```

Installed layout:

```text
sense_voice/model.onnx
sense_voice/tokens.txt
silero_vad_v4.onnx
```

Pinned files:

| Target | Source | Size | SHA-256 |
| --- | --- | ---: | --- |
| `sense_voice/model.onnx` | `https://huggingface.co/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve/2365baeacb507f821a0c8120fcee3d484dba7a07/model.int8.onnx` | `239,233,841` | `c71f0ce00bec95b07744e116345e33d8cbbe08cef896382cf907bf4b51a2cd51` |
| `sense_voice/tokens.txt` | `https://huggingface.co/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve/2365baeacb507f821a0c8120fcee3d484dba7a07/tokens.txt` | `315,894` | `f449eb28dc567533d7fa59be34e2abca8784f771850c78a47fb731a31429a1dc` |
| `silero_vad_v4.onnx` | existing pinned Silero VAD URL in `src-tauri/src/asr/models.rs` | existing | `a35ebf52fd3ce5f1469b2a36158dba761bc47b973ea3382b3186ca15b1f5af28` |

Smoke WAVs at the same revision:

```text
test_wavs/zh.wav
test_wavs/en.wav
test_wavs/ja.wav
test_wavs/ko.wav
test_wavs/yue.wav
```

Do not use Hugging Face `main` URLs. Do not use `model.onnx` for this pack; use `model.int8.onnx` and install it as `sense_voice/model.onnx`.

---

## Branches And Stop Rules

Stage S1 branch:

```powershell
git status --short --branch
git switch -c codex/android-sensevoice-s1
```

Stage S2 branch, only after S1 is accepted:

```powershell
git status --short --branch
git switch -c codex/android-sensevoice-s2
```

Stage S1 stop rule:

- Stop after the recognizer spike, AVD/standalone smoke, and findings file update.
- Do not add model pack UI.
- Do not change bubble behavior.
- Do not change downloader/manifest behavior.
- Do not open a PR for S2 work from S1.

Stage S2 start rule:

- Start only after S1 proves SenseVoice loads with 16 KB-clean runtime libraries on arm64, transcribes each zh/en/ja/ko/yue WAV, and records latency/RAM notes.

---

## Stage S1 - Recognizer Spike

### Task S1.1: Add SenseVoice Paths Without Changing Default Whisper Behavior

**Files:**

- Modify: `src-tauri/src/asr/mod.rs`
- Test: `src-tauri/src/asr/mod.rs`

- [ ] **Step 1: Write the failing path test.**

Add the SenseVoice assertions to the existing `asr_paths_resolve_from_models_dir` test:

```rust
#[test]
fn asr_paths_resolve_from_models_dir() {
    let p = AsrModelPaths::for_dir(std::path::Path::new("/data/models/verbatim-asr"));
    assert!(p.whisper_encoder.ends_with("encoder.onnx"));
    assert!(p.streaming_joiner.ends_with("joiner.onnx"));
    assert!(p.vad.ends_with("silero_vad_v4.onnx"));
    assert!(p.sense_voice_model.ends_with("sense_voice/model.onnx"));
    assert!(p.sense_voice_tokens.ends_with("sense_voice/tokens.txt"));
}
```

- [ ] **Step 2: Run the narrow test and confirm RED.**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml asr_paths_resolve_from_models_dir --lib
```

Expected: compile failure because `AsrModelPaths` has no `sense_voice_model` or `sense_voice_tokens` fields.

- [ ] **Step 3: Add the new fields.**

Extend `AsrModelPaths` without removing any existing field:

```rust
pub struct AsrModelPaths {
    pub streaming_encoder: PathBuf,
    pub streaming_decoder: PathBuf,
    pub streaming_joiner: PathBuf,
    pub streaming_tokens: PathBuf,
    pub whisper_encoder: PathBuf,
    pub whisper_decoder: PathBuf,
    pub whisper_tokens: PathBuf,
    pub sense_voice_model: PathBuf,
    pub sense_voice_tokens: PathBuf,
    pub vad: PathBuf,
}

impl AsrModelPaths {
    pub fn for_dir(dir: &Path) -> Self {
        let join = |name: &str| dir.join(name);

        Self {
            streaming_encoder: join("streaming/encoder.onnx"),
            streaming_decoder: join("streaming/decoder.onnx"),
            streaming_joiner: join("streaming/joiner.onnx"),
            streaming_tokens: join("streaming/tokens.txt"),
            whisper_encoder: join("whisper/encoder.onnx"),
            whisper_decoder: join("whisper/decoder.onnx"),
            whisper_tokens: join("whisper/tokens.txt"),
            sense_voice_model: join("sense_voice/model.onnx"),
            sense_voice_tokens: join("sense_voice/tokens.txt"),
            vad: join("silero_vad_v4.onnx"),
        }
    }
}
```

- [ ] **Step 4: Run the narrow test and confirm GREEN.**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml asr_paths_resolve_from_models_dir --lib
```

Expected: PASS.

- [ ] **Step 5: Commit the path-only change.**

Run:

```powershell
git add src-tauri/src/asr/mod.rs
git commit -m "feat(android-asr): add SenseVoice model paths"
```

### Task S1.2: Add Alternative Offline SenseVoice Constructor

**Files:**

- Modify: `src-tauri/src/asr/offline.rs`
- Test: `src-tauri/src/asr/offline.rs`

- [ ] **Step 1: Write a config-shape test before production code.**

Inside the Android `platform` module, add a test-only helper that returns the model config fields without constructing the native recognizer:

```rust
#[cfg(test)]
pub(crate) fn sense_voice_config_for_test(
    paths: &AsrModelPaths,
) -> sherpa_onnx::OfflineRecognizerConfig {
    sense_voice_config(paths)
}
```

Then add this test:

```rust
#[test]
fn sense_voice_config_uses_auto_language_itn_tokens_and_cpu() {
    let paths = AsrModelPaths::for_dir(std::path::Path::new("/models/sensevoice"));
    let config = platform::sense_voice_config_for_test(&paths);

    assert_eq!(
        config.model_config.sense_voice.model.as_deref(),
        Some("/models/sensevoice/sense_voice/model.onnx")
    );
    assert_eq!(config.model_config.sense_voice.language.as_deref(), Some("auto"));
    assert!(config.model_config.sense_voice.use_itn);
    assert_eq!(
        config.model_config.tokens.as_deref(),
        Some("/models/sensevoice/sense_voice/tokens.txt")
    );
    assert_eq!(config.model_config.provider.as_deref(), Some("cpu"));
}
```

- [ ] **Step 2: Run the narrow test and confirm RED.**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml sense_voice_config_uses_auto_language_itn_tokens_and_cpu --lib --features android-asr
```

Expected: compile failure because `sense_voice_config_for_test` and `sense_voice_config` do not exist.

- [ ] **Step 3: Add SenseVoice constructor while preserving Whisper `new()`.**

Keep `OfflineRecognizer::new(&paths, lang)` as the Whisper constructor. Add `new_sense_voice(&paths)` and a helper:

```rust
impl OfflineRecognizer {
    pub fn new(paths: &AsrModelPaths, language: &str) -> anyhow::Result<Self> {
        Self::from_config(whisper_config(paths, language))
    }

    pub fn new_sense_voice(paths: &AsrModelPaths) -> anyhow::Result<Self> {
        Self::from_config(sense_voice_config(paths))
    }

    fn from_config(config: sherpa_onnx::OfflineRecognizerConfig) -> anyhow::Result<Self> {
        let inner = sherpa_onnx::OfflineRecognizer::create(&config).ok_or_else(|| {
            anyhow::anyhow!("failed to create offline sherpa-onnx recognizer")
        })?;

        Ok(Self { inner })
    }
}

fn whisper_config(paths: &AsrModelPaths, language: &str) -> sherpa_onnx::OfflineRecognizerConfig {
    let mut config = sherpa_onnx::OfflineRecognizerConfig::default();
    config.model_config.whisper = sherpa_onnx::OfflineWhisperModelConfig {
        encoder: Some(path_to_string(&paths.whisper_encoder)),
        decoder: Some(path_to_string(&paths.whisper_decoder)),
        language: Some(language.to_string()),
        task: Some("transcribe".to_string()),
        ..Default::default()
    };
    config.model_config.tokens = Some(path_to_string(&paths.whisper_tokens));
    config.model_config.provider = Some("cpu".to_string());
    config.model_config.num_threads = 2;
    config.decoding_method = Some("greedy_search".to_string());
    config
}

fn sense_voice_config(paths: &AsrModelPaths) -> sherpa_onnx::OfflineRecognizerConfig {
    let mut config = sherpa_onnx::OfflineRecognizerConfig::default();
    config.model_config.sense_voice = sherpa_onnx::OfflineSenseVoiceModelConfig {
        model: Some(path_to_string(&paths.sense_voice_model)),
        language: Some("auto".to_string()),
        use_itn: true,
    };
    config.model_config.tokens = Some(path_to_string(&paths.sense_voice_tokens));
    config.model_config.provider = Some("cpu".to_string());
    config.model_config.num_threads = 2;
    config.decoding_method = Some("greedy_search".to_string());
    config
}
```

For the non-Android stub, add a matching method:

```rust
pub fn new_sense_voice(_paths: &AsrModelPaths) -> anyhow::Result<Self> {
    anyhow::bail!("offline sherpa-onnx recognizer is only available in Android ASR builds")
}
```

- [ ] **Step 4: Run the narrow test and existing offline test compile path.**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml sense_voice_config_uses_auto_language_itn_tokens_and_cpu --lib --features android-asr
cargo test --manifest-path src-tauri/Cargo.toml offline_whisper_transcribes_fixture --lib --features android-asr --no-run
```

Expected: config test PASS; Whisper compile path still succeeds or skips runtime fixture as before.

- [ ] **Step 5: Commit the recognizer constructor.**

Run:

```powershell
git add src-tauri/src/asr/offline.rs
git commit -m "feat(android-asr): add offline SenseVoice recognizer"
```

### Task S1.3: Add Standalone SenseVoice Smoke Binary

**Files:**

- Create: `src-tauri/src/bin/asr-sensevoice-smoke.rs`
- Create: `src-tauri/tests/fixtures/asr-sensevoice-corpus.json`

- [ ] **Step 1: Create the fixture manifest.**

Write `src-tauri/tests/fixtures/asr-sensevoice-corpus.json`:

```json
{
  "name": "sherpa-onnx-sense-voice-zh-en-ja-ko-yue-smoke",
  "source": {
    "repository": "csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17",
    "commit": "2365baeacb507f821a0c8120fcee3d484dba7a07"
  },
  "entries": [
    {
      "id": "sensevoice-zh",
      "language": "zh",
      "wav": "zh.wav",
      "sourceUrl": "https://huggingface.co/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve/2365baeacb507f821a0c8120fcee3d484dba7a07/test_wavs/zh.wav",
      "sizeBytes": 178988
    },
    {
      "id": "sensevoice-en",
      "language": "en",
      "wav": "en.wav",
      "sourceUrl": "https://huggingface.co/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve/2365baeacb507f821a0c8120fcee3d484dba7a07/test_wavs/en.wav",
      "sizeBytes": 228908
    },
    {
      "id": "sensevoice-ja",
      "language": "ja",
      "wav": "ja.wav",
      "sourceUrl": "https://huggingface.co/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve/2365baeacb507f821a0c8120fcee3d484dba7a07/test_wavs/ja.wav",
      "sizeBytes": 230444
    },
    {
      "id": "sensevoice-ko",
      "language": "ko",
      "wav": "ko.wav",
      "sourceUrl": "https://huggingface.co/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve/2365baeacb507f821a0c8120fcee3d484dba7a07/test_wavs/ko.wav",
      "sizeBytes": 147500
    },
    {
      "id": "sensevoice-yue",
      "language": "yue",
      "wav": "yue.wav",
      "sourceUrl": "https://huggingface.co/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve/2365baeacb507f821a0c8120fcee3d484dba7a07/test_wavs/yue.wav",
      "sizeBytes": 164780
    }
  ]
}
```

- [ ] **Step 2: Write the smoke binary.**

Create `src-tauri/src/bin/asr-sensevoice-smoke.rs`:

```rust
#[cfg(all(feature = "android-asr", target_os = "android"))]
fn main() -> anyhow::Result<()> {
    android::run()
}

#[cfg(not(all(feature = "android-asr", target_os = "android")))]
fn main() {
    eprintln!("asr-sensevoice-smoke is only available for Android builds with --features android-asr");
    std::process::exit(2);
}

#[cfg(all(feature = "android-asr", target_os = "android"))]
mod android {
    use anyhow::{Context, Result};
    use serde::Deserialize;
    use std::env;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::Instant;
    use verbatim_app_lib::asr::offline::OfflineRecognizer;
    use verbatim_app_lib::asr::AsrModelPaths;

    pub fn run() -> Result<()> {
        let args = Args::parse()?;
        let entries = read_manifest(&args.manifest)?;
        let manifest_dir = args
            .manifest
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let paths = AsrModelPaths::for_dir(&args.model_dir);
        let started = Instant::now();
        let mut recognizer = OfflineRecognizer::new_sense_voice(&paths)
            .context("failed to create SenseVoice recognizer")?;
        eprintln!("recognizer_init_ms={:.1}", started.elapsed().as_secs_f64() * 1000.0);

        for entry in entries {
            let wav_path = resolve_wav_path(&entry.wav, args.corpus_root.as_deref(), &manifest_dir);
            let wave = sherpa_onnx::Wave::read(wav_path.to_string_lossy().as_ref())
                .with_context(|| format!("failed to read wav {}", wav_path.display()))?;
            if wave.sample_rate() != 16_000 {
                anyhow::bail!("{} sample rate {} != 16000", wav_path.display(), wave.sample_rate());
            }
            let decode_started = Instant::now();
            let text = recognizer
                .transcribe(wave.sample_rate(), wave.samples())
                .with_context(|| format!("failed to transcribe {}", wav_path.display()))?;
            let latency_ms = decode_started.elapsed().as_secs_f64() * 1000.0;
            println!(
                "language={} wav={} samples={} latency_ms={:.1} transcript={}",
                entry.language,
                wav_path.display(),
                wave.samples().len(),
                latency_ms,
                text.replace('\n', " ")
            );
        }

        Ok(())
    }

    #[derive(Debug)]
    struct Args {
        model_dir: PathBuf,
        manifest: PathBuf,
        corpus_root: Option<PathBuf>,
    }

    impl Args {
        fn parse() -> Result<Self> {
            let mut model_dir = None;
            let mut manifest = None;
            let mut corpus_root = None;
            let mut args = env::args().skip(1);
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--model-dir" => model_dir = Some(PathBuf::from(next_value(&mut args, "--model-dir")?)),
                    "--manifest" => manifest = Some(PathBuf::from(next_value(&mut args, "--manifest")?)),
                    "--corpus-root" => corpus_root = Some(PathBuf::from(next_value(&mut args, "--corpus-root")?)),
                    "-h" | "--help" => {
                        eprintln!("Usage: asr-sensevoice-smoke --model-dir DIR --manifest FILE [--corpus-root DIR]");
                        std::process::exit(0);
                    }
                    _ => anyhow::bail!("unknown argument {arg:?}; pass --help for usage"),
                }
            }
            Ok(Self {
                model_dir: model_dir.context("--model-dir is required")?,
                manifest: manifest.context("--manifest is required")?,
                corpus_root,
            })
        }
    }

    fn next_value(args: &mut impl Iterator<Item = String>, name: &str) -> Result<String> {
        args.next().with_context(|| format!("{name} requires a value"))
    }

    #[derive(Debug, Deserialize)]
    struct ManifestEntry {
        language: String,
        wav: String,
    }

    fn read_manifest(path: &Path) -> Result<Vec<ManifestEntry>> {
        let value = serde_json::from_str::<serde_json::Value>(&fs::read_to_string(path)?)?;
        let entries = value
            .get("entries")
            .cloned()
            .with_context(|| format!("manifest {} must contain entries", path.display()))?;
        serde_json::from_value(entries)
            .with_context(|| format!("failed to decode entries in {}", path.display()))
    }

    fn resolve_wav_path(wav: &str, corpus_root: Option<&Path>, manifest_dir: &Path) -> PathBuf {
        let wav_path = PathBuf::from(wav);
        if wav_path.is_absolute() {
            wav_path
        } else if let Some(root) = corpus_root {
            root.join(wav_path)
        } else {
            manifest_dir.join(wav_path)
        }
    }
}
```

- [ ] **Step 3: Build the binary for Android x86_64 and arm64.**

Use the same `SHERPA_ONNX_LIB_DIR` locations proven by the existing G0 work:

```powershell
$env:ANDROID_NDK_HOME = "$env:LOCALAPPDATA\Android\Sdk\ndk\28.2.13676358"
$Toolchain = "$env:ANDROID_NDK_HOME\toolchains\llvm\prebuilt\windows-x86_64\bin"

$env:CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER = "$Toolchain\x86_64-linux-android26-clang.cmd"
$env:CC_x86_64_linux_android = "$Toolchain\x86_64-linux-android26-clang.cmd"
$env:AR_x86_64_linux_android = "$Toolchain\llvm-ar.exe"
$env:SHERPA_ONNX_LIB_DIR = "C:\CodexScratch\verbatim-android-asr-g0-official\android-x86_64-v1.13.3\lib"
$env:SHERPA_ONNX_ANDROID_ABI = "x86_64"
cargo build --manifest-path src-tauri\Cargo.toml --release --target x86_64-linux-android --features android-asr --bin asr-sensevoice-smoke

$env:CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER = "$Toolchain\aarch64-linux-android26-clang.cmd"
$env:CC_aarch64_linux_android = "$Toolchain\aarch64-linux-android26-clang.cmd"
$env:AR_aarch64_linux_android = "$Toolchain\llvm-ar.exe"
$env:SHERPA_ONNX_LIB_DIR = "C:\CodexScratch\verbatim-android-asr-g0-official\android-arm64-v1.13.3\lib"
$env:SHERPA_ONNX_ANDROID_ABI = "arm64-v8a"
cargo build --manifest-path src-tauri\Cargo.toml --release --target aarch64-linux-android --features android-asr --bin asr-sensevoice-smoke
```

Expected: both builds complete without changing `Cargo.lock`.

- [ ] **Step 4: Commit the smoke binary and corpus manifest.**

Run:

```powershell
git add src-tauri/src/bin/asr-sensevoice-smoke.rs src-tauri/tests/fixtures/asr-sensevoice-corpus.json
git commit -m "test(android-asr): add SenseVoice smoke harness"
```

### Task S1.4: Run AVD And Arm64 Smoke, Then Stop

**Files:**

- Create: `docs/superpowers/specs/android-asr-sensevoice-s1-findings.md`

- [ ] **Step 1: Prepare model and corpus under scratch.**

Run:

```powershell
$Scratch = "C:\CodexScratch\verbatim-sensevoice-s1"
$Model = Join-Path $Scratch "model"
$Corpus = Join-Path $Scratch "corpus"
New-Item -ItemType Directory -Force -Path "$Model\sense_voice", $Corpus | Out-Null

Invoke-WebRequest -UseBasicParsing `
  -Uri "https://huggingface.co/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve/2365baeacb507f821a0c8120fcee3d484dba7a07/model.int8.onnx" `
  -OutFile "$Model\sense_voice\model.onnx"
Invoke-WebRequest -UseBasicParsing `
  -Uri "https://huggingface.co/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve/2365baeacb507f821a0c8120fcee3d484dba7a07/tokens.txt" `
  -OutFile "$Model\sense_voice\tokens.txt"
Invoke-WebRequest -UseBasicParsing `
  -Uri "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/silero_vad_v4.onnx" `
  -OutFile "$Model\silero_vad_v4.onnx"

Get-FileHash -Algorithm SHA256 "$Model\sense_voice\model.onnx"
Get-FileHash -Algorithm SHA256 "$Model\sense_voice\tokens.txt"
Get-FileHash -Algorithm SHA256 "$Model\silero_vad_v4.onnx"
Copy-Item src-tauri\tests\fixtures\asr-sensevoice-corpus.json "$Corpus\manifest.json" -Force
foreach ($name in "zh.wav","en.wav","ja.wav","ko.wav","yue.wav") {
  Invoke-WebRequest -UseBasicParsing `
    -Uri "https://huggingface.co/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve/2365baeacb507f821a0c8120fcee3d484dba7a07/test_wavs/$name" `
    -OutFile "$Corpus\$name"
}
```

Expected SHA-256 for the downloaded model and tokens must match the table in this plan.

- [ ] **Step 2: Run on x86_64 AVD.**

Run:

```powershell
$Adb = "$env:LOCALAPPDATA\Android\Sdk\platform-tools\adb.exe"
$Remote = "/data/local/tmp/verbatim-sensevoice-s1"
$LibDir = "C:\CodexScratch\verbatim-android-asr-g0-official\android-x86_64-v1.13.3\lib"

& $Adb shell "rm -rf $Remote && mkdir -p $Remote/lib $Remote/model $Remote/corpus"
& $Adb push "src-tauri\target\x86_64-linux-android\release\asr-sensevoice-smoke" "$Remote/asr-sensevoice-smoke"
& $Adb push "$LibDir\libonnxruntime.so" "$Remote/lib/"
& $Adb push "$LibDir\libsherpa-onnx-c-api.so" "$Remote/lib/"
& $Adb push "$Model\." "$Remote/model"
& $Adb push "$Corpus\." "$Remote/corpus"
& $Adb shell "chmod 755 $Remote/asr-sensevoice-smoke $Remote/lib/*.so"
& $Adb shell "LD_LIBRARY_PATH=$Remote/lib $Remote/asr-sensevoice-smoke --model-dir $Remote/model --manifest $Remote/corpus/manifest.json --corpus-root $Remote/corpus"
```

Expected: one transcript line for each `zh`, `en`, `ja`, `ko`, and `yue`; no `FATAL`, `AndroidRuntime`, or `SIGSEGV` in logcat after the run.

- [ ] **Step 3: Run on arm64 device.**

Use the same push/run layout with:

```powershell
$LibDir = "C:\CodexScratch\verbatim-android-asr-g0-official\android-arm64-v1.13.3\lib"
& $Adb push "src-tauri\target\aarch64-linux-android\release\asr-sensevoice-smoke" "$Remote/asr-sensevoice-smoke"
```

Record device serial, model, ABI, available RAM before run, recognizer init latency, per-WAV latency, transcripts, and crash grep.

- [ ] **Step 4: Verify 16 KB alignment for the smoke artifacts.**

Run:

```powershell
bun scripts/check-android-so-alignment.ts "$LibDir"
```

Expected: `Android .so 16 KB alignment check passed`.

- [ ] **Step 5: Write findings and stop.**

Create `docs/superpowers/specs/android-asr-sensevoice-s1-findings.md` with:

```markdown
# Android SenseVoice S1 Findings

Date: 2026-06-30
Branch: `codex/android-sensevoice-s1`

## Decision

Proceed to S2 only if all rows below are PASS.

## Model

- Repo: `csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17`
- Revision: `2365baeacb507f821a0c8120fcee3d484dba7a07`
- Layout: `sense_voice/model.onnx`, `sense_voice/tokens.txt`, `silero_vad_v4.onnx`

## Device Results

| Target | ABI | 16 KB clean | Loaded | zh | en | ja | ko | yue | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| AVD | x86_64 | PASS/FAIL | PASS/FAIL | transcript | transcript | transcript | transcript | transcript | latency/RAM |
| Tab S9+ | arm64-v8a | PASS/FAIL | PASS/FAIL | transcript | transcript | transcript | transcript | transcript | latency/RAM |

## Crash Scan

Record the exact `adb logcat -d` grep used and the result.

## Checkpoint

Stop here. No bubble/UI/model-pack integration is included in S1.
```

- [ ] **Step 6: Commit S1 findings.**

Run:

```powershell
git add docs/superpowers/specs/android-asr-sensevoice-s1-findings.md
git commit -m "spike(android-asr): prove SenseVoice recognizer on Android"
```

---

## Stage S2 - Pack And Bubble Integration

Start only after the S1 findings are reviewed and accepted.

### Task S2.1: Add Engine Kind To Pack Metadata

**Files:**

- Modify: `src-tauri/src/asr/models.rs`
- Modify: `src/bindings.ts` after debug binding regeneration
- Test: `src-tauri/src/asr/models.rs`

- [ ] **Step 1: Write failing manifest tests.**

Add tests for engine kind and required targets:

```rust
#[test]
fn manifest_supports_zipformer_whisper_and_sensevoice_layouts() {
    let packs = builtin_model_packs();
    let starter = packs.iter().find(|pack| pack.id == "g3-zipformer-whisper-tiny-en").unwrap();
    let sensevoice = packs
        .iter()
        .find(|pack| pack.id == "sensevoice-multilingual-zh-en-ja-ko-yue")
        .unwrap();

    assert_eq!(starter.engine_kind, AndroidAsrEngineKind::ZipformerWhisper);
    assert_eq!(component_targets(starter), zipformer_whisper_targets());
    assert_eq!(sensevoice.engine_kind, AndroidAsrEngineKind::SenseVoice);
    assert_eq!(
        component_targets(sensevoice),
        vec![
            "sense_voice/model.onnx",
            "sense_voice/tokens.txt",
            "silero_vad_v4.onnx"
        ]
    );
}
```

- [ ] **Step 2: Run the test and confirm RED.**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml manifest_supports_zipformer_whisper_and_sensevoice_layouts --lib
```

Expected: compile failure because `AndroidAsrEngineKind` and the SenseVoice pack do not exist.

- [ ] **Step 3: Add serializable engine kind.**

Add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum AndroidAsrEngineKind {
    ZipformerWhisper,
    SenseVoice,
}
```

Add `pub engine_kind: AndroidAsrEngineKind` to both `AndroidAsrModelPack` and `AndroidAsrModelPackState`, and copy it in every state constructor.

- [ ] **Step 4: Add SenseVoice file helpers and pack.**

Add:

```rust
fn model_files_with_sense_voice() -> Vec<AndroidAsrModelFile> {
    vec![
        AndroidAsrModelFile {
            target_path: "sense_voice/model.onnx".to_string(),
            url: "https://huggingface.co/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve/2365baeacb507f821a0c8120fcee3d484dba7a07/model.int8.onnx".to_string(),
            sha256: "c71f0ce00bec95b07744e116345e33d8cbbe08cef896382cf907bf4b51a2cd51".to_string(),
            size_bytes: 239_233_841,
        },
        AndroidAsrModelFile {
            target_path: "sense_voice/tokens.txt".to_string(),
            url: "https://huggingface.co/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve/2365baeacb507f821a0c8120fcee3d484dba7a07/tokens.txt".to_string(),
            sha256: "f449eb28dc567533d7fa59be34e2abca8784f771850c78a47fb731a31429a1dc".to_string(),
            size_bytes: 315_894,
        },
        silero_vad_file(),
    ]
}
```

Append to `builtin_model_packs()`:

```rust
AndroidAsrModelPack {
    id: "sensevoice-multilingual-zh-en-ja-ko-yue".to_string(),
    display_name: "SenseVoice multilingual".to_string(),
    description: "Offline SenseVoice for Chinese, English, Japanese, Korean, and Cantonese. Final text only; no live partials.".to_string(),
    language: "auto".to_string(),
    size_mb: 229,
    engine_kind: AndroidAsrEngineKind::SenseVoice,
    files: model_files_with_sense_voice(),
}
```

Existing G3 packs must set:

```rust
engine_kind: AndroidAsrEngineKind::ZipformerWhisper,
```

- [ ] **Step 5: Update path-target helpers.**

Replace one universal `asr_model_path_targets()` helper with:

```rust
fn zipformer_whisper_targets() -> Vec<String> {
    vec![
        "streaming/encoder.onnx",
        "streaming/decoder.onnx",
        "streaming/joiner.onnx",
        "streaming/tokens.txt",
        "whisper/encoder.onnx",
        "whisper/decoder.onnx",
        "whisper/tokens.txt",
        "silero_vad_v4.onnx",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

fn sense_voice_targets() -> Vec<String> {
    vec![
        "sense_voice/model.onnx",
        "sense_voice/tokens.txt",
        "silero_vad_v4.onnx",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}
```

- [ ] **Step 6: Run model tests.**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml manifest_contains_one_extensible_pack_with_expected_layout higher_accuracy_pack_reuses_streaming_and_vad_files_with_base_whisper manifest_supports_zipformer_whisper_and_sensevoice_layouts --lib
```

Expected: all pass, with existing reuse tests still validating the two zipformer+whisper packs.

- [ ] **Step 7: Regenerate bindings after adding exported types.**

Run the repo's debug build path that exports `src/bindings.ts`:

```powershell
bun run tauri android build --debug --target x86_64 --apk --features android-asr
```

Expected: `src/bindings.ts` includes `engineKind` on `AndroidAsrModelPackState` and an `AndroidAsrEngineKind` union.

- [ ] **Step 8: Commit metadata and generated bindings.**

Run:

```powershell
git add src-tauri/src/asr/models.rs src/bindings.ts
git commit -m "feat(android-asr): add SenseVoice model pack metadata"
```

### Task S2.2: Make Runtime Paths And Sessions Engine-Aware

**Files:**

- Modify: `src-tauri/src/asr/mod.rs`
- Modify: `src-tauri/src/commands/asr.rs`
- Modify: `src-tauri/src/asr/jni_bridge.rs`
- Modify: `src-tauri/src/bin/asr-wer.rs`
- Test: `src-tauri/src/commands/asr.rs`

- [ ] **Step 1: Write failing session-construction tests.**

Add tests that use sentinel directories:

```rust
#[test]
fn session_kind_uses_streaming_when_streaming_layout_exists() {
    let temp = tempfile::tempdir().unwrap();
    write_file(temp.path().join("streaming/encoder.onnx"));
    write_file(temp.path().join("streaming/decoder.onnx"));
    write_file(temp.path().join("streaming/joiner.onnx"));
    write_file(temp.path().join("streaming/tokens.txt"));
    write_file(temp.path().join("whisper/encoder.onnx"));
    write_file(temp.path().join("whisper/decoder.onnx"));
    write_file(temp.path().join("whisper/tokens.txt"));
    write_file(temp.path().join("silero_vad_v4.onnx"));

    let paths = AsrModelPaths::for_dir(temp.path());
    assert_eq!(paths.engine_kind(), AsrEngineKind::ZipformerWhisper);
}

#[test]
fn session_kind_uses_sensevoice_when_no_streaming_tier_exists() {
    let temp = tempfile::tempdir().unwrap();
    write_file(temp.path().join("sense_voice/model.onnx"));
    write_file(temp.path().join("sense_voice/tokens.txt"));
    write_file(temp.path().join("silero_vad_v4.onnx"));

    let paths = AsrModelPaths::for_dir(temp.path());
    assert_eq!(paths.engine_kind(), AsrEngineKind::SenseVoice);
}

fn write_file(path: std::path::PathBuf) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, b"fixture").unwrap();
}
```

- [ ] **Step 2: Run tests and confirm RED.**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml session_kind_uses_ --lib
```

Expected: compile failure because `AsrEngineKind` and `engine_kind()` do not exist.

- [ ] **Step 3: Add runtime engine kind and optional streaming paths.**

In `src-tauri/src/asr/mod.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsrEngineKind {
    ZipformerWhisper,
    SenseVoice,
}

impl AsrModelPaths {
    pub fn engine_kind(&self) -> AsrEngineKind {
        if self.sense_voice_model.is_file()
            && self.sense_voice_tokens.is_file()
            && !self.streaming_encoder.is_file()
        {
            AsrEngineKind::SenseVoice
        } else {
            AsrEngineKind::ZipformerWhisper
        }
    }
}
```

Keep this as layout inference for JNI compatibility; do not add a JNI parameter unless layout inference proves ambiguous in S1/S2 tests.

- [ ] **Step 4: Split `AsrCommandSession` into streaming+offline and offline-only modes.**

In `src-tauri/src/commands/asr.rs`, change:

```rust
pub struct AsrCommandSession {
    streaming: StreamingRecognizer,
    offline: OfflineRecognizer,
    buffered_samples: Vec<f32>,
    last_partial: String,
}
```

to:

```rust
pub struct AsrCommandSession {
    mode: AsrCommandSessionMode,
    buffered_samples: Vec<f32>,
    vad: Option<crate::asr::vad::SileroVadSegmenter>,
}

enum AsrCommandSessionMode {
    ZipformerWhisper {
        streaming: StreamingRecognizer,
        offline: OfflineRecognizer,
        last_partial: String,
    },
    SenseVoice {
        offline: OfflineRecognizer,
        finalized_segments: Vec<String>,
    },
}
```

Implement `start()` as:

```rust
pub fn start(paths: AsrModelPaths, lang: &str) -> anyhow::Result<Self> {
    match paths.engine_kind() {
        AsrEngineKind::ZipformerWhisper => Ok(Self {
            mode: AsrCommandSessionMode::ZipformerWhisper {
                streaming: StreamingRecognizer::new(&paths)?,
                offline: OfflineRecognizer::new(&paths, lang)?,
                last_partial: String::new(),
            },
            buffered_samples: Vec::new(),
            vad: None,
        }),
        AsrEngineKind::SenseVoice => Ok(Self {
            mode: AsrCommandSessionMode::SenseVoice {
                offline: OfflineRecognizer::new_sense_voice(&paths)?,
                finalized_segments: Vec::new(),
            },
            buffered_samples: Vec::new(),
            vad: Some(crate::asr::vad::SileroVadSegmenter::new(&paths, SAMPLE_RATE)?),
        }),
    }
}
```

- [ ] **Step 5: Keep partials for zipformer+whisper only.**

Implement `feed_pcm()`:

```rust
pub fn feed_pcm(&mut self, frames: &[f32]) -> anyhow::Result<Vec<AsrCommandEvent>> {
    self.buffered_samples.extend_from_slice(frames);
    match &mut self.mode {
        AsrCommandSessionMode::ZipformerWhisper {
            streaming,
            last_partial,
            ..
        } => {
            if !streaming.accept_waveform(SAMPLE_RATE, frames)? {
                return Ok(Vec::new());
            }
            let text = streaming.partial_text()?;
            if text.trim().is_empty() || text == *last_partial {
                return Ok(Vec::new());
            }
            last_partial.clone_from(&text);
            Ok(vec![AsrCommandEvent::Partial { text }])
        }
        AsrCommandSessionMode::SenseVoice { offline, finalized_segments } => {
            let mut events = Vec::new();
            if let Some(vad) = &mut self.vad {
                for segment in vad.accept_waveform(frames) {
                    let text = offline.transcribe(SAMPLE_RATE, &segment.samples)?;
                    if !text.trim().is_empty() {
                        finalized_segments.push(text);
                    }
                }
            }
            Ok(events)
        }
    }
}
```

The SenseVoice branch deliberately returns no `Partial` events.

- [ ] **Step 6: Emit final for both modes.**

Implement `stop()`:

```rust
pub fn stop(&mut self) -> anyhow::Result<Vec<AsrCommandEvent>> {
    match &mut self.mode {
        AsrCommandSessionMode::ZipformerWhisper {
            streaming,
            offline,
            ..
        } => {
            let mut text = offline.transcribe(SAMPLE_RATE, &self.buffered_samples)?;
            if text.trim().is_empty() {
                text = streaming.finish()?;
            }
            Ok(vec![AsrCommandEvent::Final { text }])
        }
        AsrCommandSessionMode::SenseVoice {
            offline,
            finalized_segments,
        } => {
            if let Some(vad) = &mut self.vad {
                for segment in vad.flush() {
                    let text = offline.transcribe(SAMPLE_RATE, &segment.samples)?;
                    if !text.trim().is_empty() {
                        finalized_segments.push(text);
                    }
                }
            }
            let mut text = finalized_segments.join(" ").trim().to_string();
            if text.is_empty() {
                text = offline.transcribe(SAMPLE_RATE, &self.buffered_samples)?;
            }
            Ok(vec![AsrCommandEvent::Final { text }])
        }
    }
}
```

- [ ] **Step 7: Keep JNI event mapping unchanged.**

`jni_bridge.rs` can continue dispatching `Partial` and `Final`; the SenseVoice mode emits only `Final`. Do not add a new JNI callback unless a test proves the Kotlin service needs one.

- [ ] **Step 8: Update `asr-wer.rs` to skip streaming metrics for SenseVoice packs.**

Add a report field such as `engineKind`, make streaming fields `Option`, and for SenseVoice call only `OfflineRecognizer::new_sense_voice(&paths)`. Preserve existing JSON fields for zipformer+whisper reports.

- [ ] **Step 9: Run Rust checks.**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml session_kind_uses_ --lib
cargo test --manifest-path src-tauri/Cargo.toml asr_command_session_emits_final_for_fixture_frames --lib --features android-asr --no-run
cargo test --manifest-path src-tauri/Cargo.toml --lib --no-default-features
```

Expected: unit tests pass; Android fixture tests compile under `--features android-asr` without requiring host runtime execution.

- [ ] **Step 10: Commit engine-aware runtime.**

Run:

```powershell
git add src-tauri/src/asr/mod.rs src-tauri/src/commands/asr.rs src-tauri/src/asr/jni_bridge.rs src-tauri/src/bin/asr-wer.rs
git commit -m "feat(android-asr): route offline-only SenseVoice sessions"
```

### Task S2.3: Make Android Bubble Model Requirements Engine-Aware

**Files:**

- Modify: `src-tauri/gen/android/app/src/main/java/com/galaxyruler/verbatim/FloatingBubbleService.kt`
- Modify: `src-tauri/gen/android/app/src/main/java/com/galaxyruler/verbatim/EngineModelSelectionStore.kt`
- Modify: `src-tauri/gen/android/app/src/test/java/com/galaxyruler/verbatim/EngineModelSelectionTest.kt`

- [ ] **Step 1: Write failing Kotlin unit tests for both layouts.**

Extend `EngineModelSelectionTest.kt`:

```kotlin
@Test
fun engineModelInstallationAcceptsSenseVoiceLayoutWithoutStreamingFiles() {
  val fixture = EngineSelectionFixture("sensevoice")
  val packDir = File(fixture.appDataRoot, "models/android-asr/sensevoice-multilingual-zh-en-ja-ko-yue")
  fixture.setEngineModelId("sensevoice-multilingual-zh-en-ja-ko-yue")

  assertFalse(
    EngineModelSelectionStore.isEngineModelInstalled(
      fixture.context,
      EngineModelSelectionStore.requiredFilesForPack(fixture.context),
    ),
  )

  arrayOf("sense_voice/model.onnx", "sense_voice/tokens.txt", "silero_vad_v4.onnx").forEach {
    File(packDir, it).also { file ->
      file.parentFile?.mkdirs()
      file.writeText("fixture")
    }
  }

  assertTrue(
    EngineModelSelectionStore.isEngineModelInstalled(
      fixture.context,
      EngineModelSelectionStore.requiredFilesForPack(fixture.context),
    ),
  )
  fixture.cleanup()
}
```

Use a small fixture helper if it reduces duplication in this test file.

- [ ] **Step 2: Run Kotlin unit tests and confirm RED.**

Run:

```powershell
bun run android:test:unit -- --tests com.galaxyruler.verbatim.EngineModelSelectionTest
```

Expected: compile failure because `requiredFilesForPack` does not exist.

- [ ] **Step 3: Add required-file selection in Kotlin.**

In `EngineModelSelectionStore.kt`:

```kotlin
private val ZIPFORMER_WHISPER_REQUIRED_FILES = arrayOf(
  "streaming/encoder.onnx",
  "streaming/decoder.onnx",
  "streaming/joiner.onnx",
  "streaming/tokens.txt",
  "whisper/encoder.onnx",
  "whisper/decoder.onnx",
  "whisper/tokens.txt",
  "silero_vad_v4.onnx",
)

private val SENSEVOICE_REQUIRED_FILES = arrayOf(
  "sense_voice/model.onnx",
  "sense_voice/tokens.txt",
  "silero_vad_v4.onnx",
)

fun requiredFilesForPack(context: Context): Array<String> =
  if (engineModelId(context) == "sensevoice-multilingual-zh-en-ja-ko-yue") {
    SENSEVOICE_REQUIRED_FILES
  } else {
    ZIPFORMER_WHISPER_REQUIRED_FILES
  }
```

In `FloatingBubbleService.kt`, replace `REQUIRED_ENGINE_MODEL_FILES` usage with:

```kotlin
private fun isEngineModelInstalled(): Boolean =
  EngineModelSelectionStore.isEngineModelInstalled(
    this,
    EngineModelSelectionStore.requiredFilesForPack(this),
  )
```

Remove the ASR hardcoded array from `FloatingBubbleService.kt` after the store owns both layouts.

- [ ] **Step 4: Run Kotlin unit tests.**

Run:

```powershell
bun run android:test:unit -- --tests com.galaxyruler.verbatim.EngineModelSelectionTest
```

Expected: PASS.

- [ ] **Step 5: Confirm bubble state for no-partial engine.**

Keep `startEngineDictation()` setting `BubbleState.RECORDING` while the mic records. `runDebugWavFeedLoop()` already switches to `BubbleState.TRANSCRIBING` before `nativeAsrStop()`. For live mic stop, make `stopEngineDictation()` set `BubbleState.TRANSCRIBING` before calling `nativeAsrStop()`:

```kotlin
private fun stopEngineDictation() {
  stopEngineCapture()
  livePartialText = null
  bubbleState = BubbleState.TRANSCRIBING
  bubbleView?.let { renderBubble(it) }
  logAsr("nativeAsrStop called")
  if (!nativeAsrStop()) {
    logAsr("nativeAsrStop returned false")
    stopMicrophoneForeground()
    showFailure(R.string.bubble_listen_failed, null)
  } else {
    logAsr("nativeAsrStop returned true")
  }
}
```

Do not special-case SenseVoice partial text. No partial callback should arrive from Rust for SenseVoice.

- [ ] **Step 6: Commit Kotlin integration.**

Run:

```powershell
git add src-tauri/gen/android/app/src/main/java/com/galaxyruler/verbatim/FloatingBubbleService.kt src-tauri/gen/android/app/src/main/java/com/galaxyruler/verbatim/EngineModelSelectionStore.kt src-tauri/gen/android/app/src/test/java/com/galaxyruler/verbatim/EngineModelSelectionTest.kt
git commit -m "feat(android): support offline-only ASR model layout"
```

### Task S2.4: Add Pack i18n In EN And 19 Locales

**Files:**

- Modify: `src/i18n/locales/en/translation.json`
- Modify: `src/i18n/locales/{ar,bg,cs,de,es,fr,he,it,ja,ko,pl,pt,ru,sv,tr,uk,vi,zh,zh-TW}/translation.json`

- [ ] **Step 1: Add English keys.**

Under `android.models.packs`, add:

```json
"sensevoice-multilingual-zh-en-ja-ko-yue": {
  "displayName": "SenseVoice multilingual",
  "description": "Offline Chinese, English, Japanese, Korean, and Cantonese. Final text only; no live partials."
}
```

- [ ] **Step 2: Add the same key shape to all 19 non-English locale files.**

Use localized strings where the repo already has high-quality coverage. If a locale is partially English in this Android section, copy the English value so `bun run check:translations` passes without adding missing keys.

- [ ] **Step 3: Run translation check.**

Run:

```powershell
bun run check:translations
```

Expected: PASS, including RTL untranslated-value guard for `ar` and `he`.

- [ ] **Step 4: Commit i18n.**

Run:

```powershell
git add src/i18n/locales
git commit -m "feat(android): localize SenseVoice model pack"
```

### Task S2.5: Regenerate, Build, And Guard Android

**Files:**

- Modify: generated files only if commands/types changed (`src/bindings.ts`)
- No lockfile changes expected

- [ ] **Step 1: Run focused checks.**

Run:

```powershell
bun run check:translations
bun run lint
cargo test --manifest-path src-tauri/Cargo.toml --lib --no-default-features
bun run android:test:unit
```

Expected: all pass.

- [ ] **Step 2: Run Android build with ASR feature and regen guard.**

Run:

```powershell
bun run tauri android build --debug --target x86_64 --apk --features android-asr
bun scripts/check-android-so-alignment.ts src-tauri/gen/android/app/build/outputs/apk
```

Expected: APK builds, 16 KB guard passes.

- [ ] **Step 3: Run Android E2E smoke.**

Run:

```powershell
bun run android:e2e:maestro
```

Expected: existing G0-G4 Android flows remain green. If Maestro cannot run locally, record the exact missing dependency or emulator state in the PR testing section.

- [ ] **Step 4: Run no-streaming engine WAV smoke on AVD.**

Install the debug APK, select the SenseVoice pack, then run the existing debug broadcast path with `zh.wav` and `ja.wav`:

```powershell
$Adb = "$env:LOCALAPPDATA\Android\Sdk\platform-tools\adb.exe"
$Pkg = "com.galaxyruler.verbatim"
$Remote = "/data/local/tmp/verbatim-sensevoice-s2"
& $Adb shell "rm -rf $Remote && mkdir -p $Remote/wav"
& $Adb push "C:\CodexScratch\verbatim-sensevoice-s1\corpus\zh.wav" "$Remote/wav/zh.wav"
& $Adb push "C:\CodexScratch\verbatim-sensevoice-s1\corpus\ja.wav" "$Remote/wav/ja.wav"
& $Adb shell "am broadcast -a com.galaxyruler.verbatim.action.DEBUG_ENGINE_WAV_SMOKE -n $Pkg/.DebugInsertProbeReceiver --es wav_path $Remote/wav/zh.wav"
& $Adb shell "am broadcast -a com.galaxyruler.verbatim.action.DEBUG_ENGINE_WAV_SMOKE -n $Pkg/.DebugInsertProbeReceiver --es wav_path $Remote/wav/ja.wav"
```

Expected log signatures:

```text
nativeAsrStart returned true
nativeAsrStop returned true
onFinal callback len=<positive number>
```

Expected absence:

```text
onPartial callback
FATAL
AndroidRuntime
SIGSEGV
```

- [ ] **Step 5: Commit verification-only generated changes if any.**

Run:

```powershell
git status --short
```

If `src/bindings.ts` changed in this task and was not already committed:

```powershell
git add src/bindings.ts
git commit -m "chore(android-asr): refresh bindings for SenseVoice pack metadata"
```

### Task S2.6: Physical Device Merge Gate

**Device:** Samsung Tab S9+.

- [ ] **Step 1: Install debug APK on the device.**

Record package version, git SHA, and ABI.

- [ ] **Step 2: Download the SenseVoice pack through the Models tab.**

Expected:

- Pack displays as multilingual/offline/final-only.
- Download, verify, and install phases complete.
- The pack auto-selects when active slot is empty or can be manually selected.
- Installed layout under app data contains:

```text
models/android-asr/sensevoice-multilingual-zh-en-ja-ko-yue/sense_voice/model.onnx
models/android-asr/sensevoice-multilingual-zh-en-ja-ko-yue/sense_voice/tokens.txt
models/android-asr/sensevoice-multilingual-zh-en-ja-ko-yue/silero_vad_v4.onnx
```

- [ ] **Step 3: Run engine WAV smoke with non-English fixtures.**

Use `zh.wav` and `ja.wav`.

Expected:

- `nativeAsrStart returned true`.
- Final text is multilingual and plausibly aligned to the fixture language.
- No crash.
- No `onPartial callback` for SenseVoice.

- [ ] **Step 4: Run floating bubble flow.**

Expected:

- Bubble records.
- Bubble shows `Transcribing` after stop.
- Bubble inserts final text.
- Native history keeps the final text.

- [ ] **Step 5: Record RAM and latency.**

Record:

```powershell
adb shell dumpsys meminfo com.galaxyruler.verbatim
adb logcat -d | Select-String -Pattern "VerbatimASR|FloatingBubble|nativeAsrStart|nativeAsrStop|onFinal|FATAL|AndroidRuntime|SIGSEGV"
```

- [ ] **Step 6: Write S2 evidence note.**

Create `docs/superpowers/specs/android-asr-sensevoice-s2-findings.md` with the device steps, transcripts, latency, RAM, and crash scan.

- [ ] **Step 7: Commit S2 evidence.**

Run:

```powershell
git add docs/superpowers/specs/android-asr-sensevoice-s2-findings.md
git commit -m "test(android-asr): record SenseVoice device evidence"
```

---

## PR Requirements

- Read `.github/PULL_REQUEST_TEMPLATE.md` before opening any PR.
- S1 PR describes a spike only, includes the findings table, and states that no UI/bubble integration is included.
- S2 PR links S1 findings and includes:
  - Summary of engine-kind switch.
  - Confirmation that existing `g3-zipformer-whisper-tiny-en` and `g3-zipformer-whisper-base-en` still stream partials.
  - Confirmation that SenseVoice emits final-only text.
  - Translation check result.
  - Android unit/e2e result.
  - 16 KB guard result.
  - Tab S9+ evidence.
- The PR template's Human Written Description must be supplied by the human contributor before the PR is opened; do not invent their voice.
- All checkboxes in the PR body that apply to AI assistance and testing must be filled with real evidence, not generic wording.

## Final Verification Matrix

Run before S2 merge:

```powershell
bun run check:translations
bun run lint
cargo test --manifest-path src-tauri/Cargo.toml --lib --no-default-features
bun run android:test:unit
bun run tauri android build --debug --target x86_64 --apk --features android-asr
bun scripts/check-android-so-alignment.ts src-tauri/gen/android/app/build/outputs/apk
bun run android:e2e:maestro
```

Manual gates:

- S1 AVD and arm64 smoke over zh/en/ja/ko/yue.
- S2 Tab S9+ pack download, selection, `zh.wav`, `ja.wav`, final insertion, no partials, no crash.
- Existing zipformer+whisper pack smoke still produces partial and final events.

## Self-Review

- S1 proves the recognizer and stops before integration.
- S2 is cleanly behind engine kind and optional streaming.
- SenseVoice assets are pinned to immutable commit `2365baeacb507f821a0c8120fcee3d484dba7a07`.
- Existing G3 pack layout and tests remain explicit.
- The no-streaming path emits only final events.
- English plus 19 locale key coverage is included.
- PR requirements respect the repo template and do not invent the human-written section.
