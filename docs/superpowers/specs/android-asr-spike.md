# Android ASR G0 Spike Findings

Date: 2026-06-25

Plan: `docs/superpowers/plans/2026-06-25-verbatim-android-phase2-ondevice-engine.md`

Scope: Task 1 only. Stop point: Task 1 Step 5 decision checkpoint.

## Decision

Do not proceed to Task 2 yet.

The latest verified sherpa-onnx Android engine archive has 16 KB-aligned shared libraries, and a scratch `sherpa-rs` crate can cross-link against those libraries for Android release builds. However, the required AVD smoke transcription does not pass: `WhisperRecognizer::new` segfaults on `Verbatim_API_35_x86_64` before the WAV is read.

This fails Gate G0 as written. The likely integration issue is a binding/runtime version mismatch: `sherpa-rs 0.6.8` still vendors/builds against sherpa-onnx `v1.12.9` in crates.io, while the first confirmed 16 KB-aligned runtime checked here is sherpa-onnx `v1.13.3`.

## Pinned Candidate

- sherpa-onnx: `v1.13.3`
- Android archive: `sherpa-onnx-v1.13.3-android.tar.bz2`
- Published: 2026-06-15
- ONNX Runtime in archive: `1.24.3`
- Evidence:
  - `gh api repos/k2-fsa/sherpa-onnx/releases/tags/v1.13.3`
  - `gh api repos/k2-fsa/sherpa-onnx/pulls/3617`
  - `llvm-strings libonnxruntime.so` showed `VERS_1.24.3`
  - PR 3617 release note says Android default ONNX Runtime is `v1.24.3`

Do not use the `sherpa-rs 0.6.8` default binary downloader for this gate. Its `sherpa-rs-sys/dist.json` points Android downloads at sherpa-onnx `v1.12.9`, which predates the 16 KB fix tracked in k2-fsa/sherpa-onnx#3291.

## Step 1 - Pin 16 KB-Clean sherpa-onnx/ORT

Commands:

```powershell
gh api repos/k2-fsa/sherpa-onnx/releases?per_page=20
gh api repos/k2-fsa/sherpa-onnx/releases/tags/v1.13.3
gh api repos/k2-fsa/sherpa-onnx/issues/3291
gh api repos/k2-fsa/sherpa-onnx/pulls/3617
gh release download v1.13.3 -R k2-fsa/sherpa-onnx -p 'sherpa-onnx-v1.13.3-android.tar.bz2'
tar -xjf sherpa-onnx-v1.13.3-android.tar.bz2
```

Result: Found a candidate aligned prebuilt, `sherpa-onnx-v1.13.3-android.tar.bz2`, with Android `libonnxruntime.so` from ORT `1.24.3`.

## Step 2 - Cross-Compile sherpa-rs for Android

Scratch directory:

```text
C:\CodexScratch\verbatim-android-asr-g0\sherpa-rs-android-link-smoke
```

Pinned Rust crate:

```toml
sherpa-rs = { version = "=0.6.8", default-features = false }
```

Android toolchain:

```text
ANDROID_NDK_HOME=C:\Users\Admin\AppData\Local\Android\Sdk\ndk\28.2.13676358
```

Important build notes:

- Debug Android cross-compile fails because `sherpa-rs-sys 0.6.8` adds `-lmsvcrtd` when the Windows-host build script has debug assertions enabled.
- Release Android cross-compile succeeds when `SHERPA_LIB_PATH` points at copied `v1.13.3` ABI-specific libs.
- Bindgen on this Windows NDK layout needs explicit sysroot and clang builtin include paths.

Successful command:

```powershell
$env:SHERPA_LIB_PATH = 'C:\CodexScratch\verbatim-android-asr-g0\android-arm64-v1.13.3'
cargo build --release --target aarch64-linux-android
```

Result: PASS for release cross-link.

## Step 3 - Verify Alignment

Command:

```powershell
$objdump = "$env:LOCALAPPDATA\Android\Sdk\ndk\28.2.13676358\toolchains\llvm\prebuilt\windows-x86_64\bin\llvm-objdump.exe"
& $objdump -p <lib>.so
```

Archive alignment result for `sherpa-onnx-v1.13.3-android.tar.bz2`:

| ABI | Library | Min LOAD align | Result |
| --- | --- | --- | --- |
| arm64-v8a | libonnxruntime.so | 2**14 | PASS |
| arm64-v8a | libsherpa-onnx-c-api.so | 2**14 | PASS |
| arm64-v8a | libsherpa-onnx-cxx-api.so | 2**14 | PASS |
| arm64-v8a | libsherpa-onnx-jni.so | 2**14 | PASS |
| armeabi-v7a | libonnxruntime.so | 2**14 | PASS |
| armeabi-v7a | libsherpa-onnx-c-api.so | 2**14 | PASS |
| armeabi-v7a | libsherpa-onnx-cxx-api.so | 2**14 | PASS |
| armeabi-v7a | libsherpa-onnx-jni.so | 2**14 | PASS |
| x86 | libonnxruntime.so | 2**14 | PASS |
| x86 | libsherpa-onnx-c-api.so | 2**14 | PASS |
| x86 | libsherpa-onnx-cxx-api.so | 2**14 | PASS |
| x86 | libsherpa-onnx-jni.so | 2**14 | PASS |
| x86_64 | libonnxruntime.so | 2**14 | PASS |
| x86_64 | libsherpa-onnx-c-api.so | 2**14 | PASS |
| x86_64 | libsherpa-onnx-cxx-api.so | 2**14 | PASS |
| x86_64 | libsherpa-onnx-jni.so | 2**14 | PASS |

Scratch aarch64 release output alignment:

| File | Min LOAD align | Result |
| --- | --- | --- |
| libonnxruntime.so | 2**14 | PASS |
| libsherpa-onnx-c-api.so | 2**14 | PASS |
| deps/libonnxruntime.so | 2**14 | PASS |
| deps/libsherpa-onnx-c-api.so | 2**14 | PASS |
| deps/libsherpa_rs-65211738a36fa295.so | 2**14 | PASS |

## Step 4 - AVD Smoke Transcription

AVD:

```text
Verbatim_API_35_x86_64
```

Model:

```text
sherpa-onnx-whisper-tiny.en.tar.bz2
```

Fixture:

```text
sherpa-onnx-whisper-tiny.en/test_wavs/0.wav
```

Commands:

```powershell
cargo build --release --target x86_64-linux-android
adb shell "rm -rf /data/local/tmp/verbatim-asr-smoke && mkdir -p /data/local/tmp/verbatim-asr-smoke/lib /data/local/tmp/verbatim-asr-smoke/model /data/local/tmp/verbatim-asr-smoke/wav"
adb push <binary> /data/local/tmp/verbatim-asr-smoke/
adb push libonnxruntime.so /data/local/tmp/verbatim-asr-smoke/lib/
adb push libsherpa-onnx-c-api.so /data/local/tmp/verbatim-asr-smoke/lib/
adb push tiny.en-encoder.int8.onnx /data/local/tmp/verbatim-asr-smoke/model/
adb push tiny.en-decoder.int8.onnx /data/local/tmp/verbatim-asr-smoke/model/
adb push tiny.en-tokens.txt /data/local/tmp/verbatim-asr-smoke/model/
adb push 0.wav /data/local/tmp/verbatim-asr-smoke/wav/
adb shell "cd /data/local/tmp/verbatim-asr-smoke && LD_LIBRARY_PATH=/data/local/tmp/verbatim-asr-smoke/lib ./sherpa-rs-android-link-smoke /data/local/tmp/verbatim-asr-smoke/model /data/local/tmp/verbatim-asr-smoke/wav/0.wav"
```

Observed output:

```text
creating recognizer
Segmentation fault
```

Result: FAIL. The crash happens during `WhisperRecognizer::new`, before reading the WAV or calling `transcribe`.

## Step 5 - Checkpoint

Gate G0 is blocked. Do not start Task 2 or G1.

Recommended maintainer decision:

1. Patch or fork `sherpa-rs-sys` so generated bindings and vendored headers match sherpa-onnx `v1.13.3` or newer, then repeat Task 1 Step 2-4.
2. Alternatively, use sherpa-onnx's official Rust API if it now supersedes `sherpa-rs`; repeat the same 16 KB alignment and AVD smoke checks before wiring the plugin.
3. Do not ship a non-16 KB-compliant prebuilt or rely on the `sherpa-rs 0.6.8` default `v1.12.9` binary download.

## Amendment 1 Re-Spike - Official sherpa-onnx crate

Date: 2026-06-25

Plan amendment: Use the official `sherpa-onnx` crate instead of third-party `sherpa-rs`.

### Step 2 - Cross-Compile Official sherpa-onnx Crate for Android

Scratch directory:

```text
C:\CodexScratch\verbatim-android-asr-g0-official\official-sherpa-onnx-android-smoke
```

Pinned Rust crate:

```toml
sherpa-onnx = { version = "=1.13.3", default-features = false, features = ["shared"] }
```

Android toolchain:

```text
ANDROID_NDK_HOME=C:\Users\Admin\AppData\Local\Android\Sdk\ndk\28.2.13676358
```

Build commands:

```powershell
$env:SHERPA_ONNX_LIB_DIR = 'C:\CodexScratch\verbatim-android-asr-g0-official\android-arm64-v1.13.3\lib'
cargo build --release --target aarch64-linux-android

$env:SHERPA_ONNX_LIB_DIR = 'C:\CodexScratch\verbatim-android-asr-g0-official\android-x86_64-v1.13.3\lib'
cargo build --release --target x86_64-linux-android
```

Result: PASS for both `aarch64-linux-android` and `x86_64-linux-android`.

API surface confirmed in `sherpa-onnx 1.13.3`:

- Offline ASR: `OfflineRecognizer`, `OfflineRecognizerConfig`, `OfflineWhisperModelConfig`
- Streaming ASR: `OnlineRecognizer`, `OnlineRecognizerConfig`, `OnlineTransducerModelConfig`
- VAD: `VoiceActivityDetector`, `VadModelConfig`, `SileroVadModelConfig`
- WAV I/O: `Wave::read`

The official crate's build script does not auto-download Android archives, but it honors `SHERPA_ONNX_LIB_DIR` for Android targets. The scratch build used `SHERPA_ONNX_LIB_DIR` pointed at the verified `v1.13.3` ABI-specific `lib` directories copied from `sherpa-onnx-v1.13.3-android.tar.bz2`.

### Step 3 - Verify Alignment

Command:

```powershell
$objdump = "$env:LOCALAPPDATA\Android\Sdk\ndk\28.2.13676358\toolchains\llvm\prebuilt\windows-x86_64\bin\llvm-objdump.exe"
& $objdump -p <lib>.so
```

Runtime library alignment:

| ABI | Library | Min LOAD align | Result |
| --- | --- | --- | --- |
| arm64-v8a | libonnxruntime.so | 2**14 | PASS |
| arm64-v8a | libsherpa-onnx-c-api.so | 2**14 | PASS |
| x86_64 | libonnxruntime.so | 2**14 | PASS |
| x86_64 | libsherpa-onnx-c-api.so | 2**14 | PASS |

Result: PASS.

### Step 4 - AVD Smoke Transcription

AVD:

```text
Verbatim_API_35_x86_64
```

Model:

```text
sherpa-onnx-whisper-tiny.en.tar.bz2
```

Fixture:

```text
sherpa-onnx-whisper-tiny.en/test_wavs/0.wav
```

Command:

```powershell
adb shell "cd /data/local/tmp/verbatim-asr-official-smoke && LD_LIBRARY_PATH=/data/local/tmp/verbatim-asr-official-smoke/lib ./official-sherpa-onnx-android-smoke /data/local/tmp/verbatim-asr-official-smoke/model /data/local/tmp/verbatim-asr-official-smoke/wav/0.wav"
```

Observed output:

```text
creating recognizer
reading wav
transcribing 16000 Hz, 106000 samples
transcription returned
After early nightfall, the yellow lamps would light up here and there the squalid quarter of the brothels.
```

Result: PASS. The official crate removes the previous ABI-skew crash and returns non-empty text on the AVD.

### Step 5 - Checkpoint

Task 1 is green under Amendment 1. Proceed to Task 2 using the official `sherpa-onnx` crate.
