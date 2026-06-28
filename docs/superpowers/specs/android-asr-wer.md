# Android ASR WER Harness

This is the repeatable Gate G2 harness for on-device ASR accuracy and latency. It runs the real Android `android-asr` Rust recognizers over a labeled WAV corpus, prints a human summary to stderr, and emits a machine-readable JSON report to stdout and optionally to `--json-out`.

## What It Measures

- Offline Whisper WER, using normalized word-level Levenshtein distance.
- Streaming Zipformer WER, using the streaming final text.
- First non-empty streaming partial latency per file.
- Streaming total latency, offline latency, and combined total latency per file.

Normalization is intentionally simple and repeatable: lowercase, strip punctuation, split/collapse whitespace. The raw hypotheses are still included in the JSON report. Known sherpa gotcha: Whisper often emits a leading space; that stays visible in the raw hypothesis but is ignored by WER normalization.

The timing excludes model-pack download and recognizer construction. It measures decode work after the model files are already present on the device.

## Corpus

The default smoke corpus manifest is:

```text
src-tauri/tests/fixtures/asr-wer-corpus.json
```

It uses two pinned 16 kHz mono fixtures from `csukuangfj/sherpa-onnx-whisper-tiny.en` at commit `d026532c022fa99fd789d6b32446a1df7b6bfc43`. The upstream `test_wavs/8k.wav` file is omitted because this harness expects 16 kHz input.

Fetch and verify the WAVs on the host:

```powershell
$CorpusDir = "C:\CodexScratch\verbatim-asr-wer-corpus"
$Manifest = "src-tauri\tests\fixtures\asr-wer-corpus.json"
New-Item -ItemType Directory -Force -Path $CorpusDir | Out-Null
$Corpus = Get-Content -Raw $Manifest | ConvertFrom-Json
foreach ($Entry in $Corpus.entries) {
  $Out = Join-Path $CorpusDir $Entry.wav
  Invoke-WebRequest -UseBasicParsing -Uri $Entry.sourceUrl -OutFile $Out
  $Actual = (Get-FileHash -Algorithm SHA256 $Out).Hash.ToLowerInvariant()
  if ($Actual -ne $Entry.sha256) {
    throw "Hash mismatch for $($Entry.wav): $Actual"
  }
}
Copy-Item $Manifest (Join-Path $CorpusDir "manifest.json") -Force
```

## Build The Harness

Use the same sherpa-onnx v1.13.3 Android runtime libraries as the app build. The prior G0/G1 runs used:

```powershell
$env:ANDROID_NDK_HOME = "$env:LOCALAPPDATA\Android\Sdk\ndk\28.2.13676358"
$Toolchain = "$env:ANDROID_NDK_HOME\toolchains\llvm\prebuilt\windows-x86_64\bin"
$env:CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER = "$Toolchain\aarch64-linux-android26-clang.cmd"
$env:CC_aarch64_linux_android = "$Toolchain\aarch64-linux-android26-clang.cmd"
$env:AR_aarch64_linux_android = "$Toolchain\llvm-ar.exe"
$env:SHERPA_ONNX_LIB_DIR = "C:\CodexScratch\verbatim-android-asr-g0-official\android-arm64-v1.13.3\lib"
$env:SHERPA_ONNX_ANDROID_ABI = "arm64-v8a"

cargo build --manifest-path src-tauri\Cargo.toml --release --target aarch64-linux-android --features android-asr --bin asr-wer
```

For the x86_64 emulator smoke, switch these values:

```powershell
$env:CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER = "$Toolchain\x86_64-linux-android26-clang.cmd"
$env:CC_x86_64_linux_android = "$Toolchain\x86_64-linux-android26-clang.cmd"
$env:AR_x86_64_linux_android = "$Toolchain\llvm-ar.exe"
$env:SHERPA_ONNX_LIB_DIR = "C:\CodexScratch\verbatim-android-asr-g0-official\android-x86_64-v1.13.3\lib"
$env:SHERPA_ONNX_ANDROID_ABI = "x86_64"

cargo build --manifest-path src-tauri\Cargo.toml --release --target x86_64-linux-android --features android-asr --bin asr-wer
```

## Run On A Physical Arm64 Device

Install a debug Verbatim APK, download/select the `g3-zipformer-whisper-tiny-en` pack in the app, and keep the package debuggable so `run-as` can read the app data directory.

Important model path gotcha: Android model packs live under `applicationInfo.dataDir/models/android-asr/<id>`, not under `filesDir`. On a typical install that resolves to:

```text
/data/user/0/com.galaxyruler.verbatim/models/android-asr/g3-zipformer-whisper-tiny-en
```

Verify the app-private pack without copying anything:

```powershell
$Adb = "$env:LOCALAPPDATA\Android\Sdk\platform-tools\adb.exe"
$Pkg = "com.galaxyruler.verbatim"
& $Adb shell "run-as $Pkg sh -c 'find models/android-asr/g3-zipformer-whisper-tiny-en -maxdepth 3 -type f | sort'"
```

On Android 16 / SM-X810, `run-as` can read `applicationInfo.dataDir`, but the app UID could not execute a harness binary placed in `/data/local/tmp` (`Permission denied`). The repeatable runner path below executes as the `shell` user and uses a temp model copy with the same layout.

Push the harness, shared libraries, corpus, and model layout:

```powershell
$Adb = "$env:LOCALAPPDATA\Android\Sdk\platform-tools\adb.exe"
$Remote = "/data/local/tmp/verbatim-asr-wer"
$CorpusDir = "C:\CodexScratch\verbatim-asr-wer-corpus"
$LibDir = "C:\CodexScratch\verbatim-android-asr-g0-official\android-arm64-v1.13.3\lib"
$ModelDir = "C:\CodexScratch\verbatim-asr-wer-model"

& $Adb shell "rm -rf $Remote && mkdir -p $Remote/lib $Remote/corpus $Remote/model"
& $Adb push "src-tauri\target\aarch64-linux-android\release\asr-wer" "$Remote/asr-wer"
& $Adb push "$LibDir\libonnxruntime.so" "$Remote/lib/"
& $Adb push "$LibDir\libsherpa-onnx-c-api.so" "$Remote/lib/"
& $Adb push "$CorpusDir\manifest.json" "$Remote/corpus/manifest.json"
& $Adb push "$CorpusDir\0.wav" "$Remote/corpus/0.wav"
& $Adb push "$CorpusDir\1.wav" "$Remote/corpus/1.wav"
& $Adb push "$ModelDir\." "$Remote/model"
& $Adb shell "chmod 755 $Remote/asr-wer $Remote/lib/*.so"
```

If you do not already have `C:\CodexScratch\verbatim-asr-wer-model`, create it from the G3 per-file assets by arranging:

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

Run the harness:

```powershell
& $Adb shell "LD_LIBRARY_PATH=$Remote/lib $Remote/asr-wer --model-dir $Remote/model --manifest $Remote/corpus/manifest.json --corpus-root $Remote/corpus --language en --json-out $Remote/asr-wer-report.json"
& $Adb pull "$Remote/asr-wer-report.json" ".\android-asr-wer-report.arm64.json"
```

Record the human summary and attach the JSON report to the G2 PR evidence. The Samsung Tab S9+ physical run is the closure evidence; emulator numbers are only a smoke baseline.

## Run On The x86_64 Emulator

Use the same steps with the x86_64 build output and x86_64 sherpa libraries:

```powershell
$LibDir = "C:\CodexScratch\verbatim-android-asr-g0-official\android-x86_64-v1.13.3\lib"
& $Adb push "src-tauri\target\x86_64-linux-android\release\asr-wer" "$Remote/asr-wer"
& $Adb push "$LibDir\libonnxruntime.so" "$Remote/lib/"
& $Adb push "$LibDir\libsherpa-onnx-c-api.so" "$Remote/lib/"
```

Then use the same shell/temp-model run command. x86_64 emulator WER can catch regressions in model paths and recognizer wiring, but it cannot establish arm64 latency, RAM, thermal behavior, or physical-device G2 acceptance.

## Local SM-X810 Smoke

Observed on 2026-06-28 with `SM-X810` (`arm64-v8a`), temp model layout under `/data/local/tmp/verbatim-asr-wer/model`, and the two-file smoke corpus:

- Offline WER: 4.55% (3 errors / 66 reference words)
- Streaming WER: 9.09% (6 errors / 66 reference words)
- Average first partial latency: 61.8 ms
- Average combined streaming + offline latency: 1186.5 ms

The JSON artifact for this local run was saved at `.local-builds/android-asr-wer/asr-wer-report.SM-X810.arm64.json`.

## JSON Shape

The report contains `files[]` with raw reference, streaming hypothesis, offline hypothesis, WER breakdowns, and latency fields:

```json
{
  "aggregate": {
    "offlineWer": { "errors": 0, "referenceWords": 64, "wer": 0.0 },
    "avgStreamingFirstPartialLatencyMs": 92.4,
    "avgTotalLatencyMs": 1830.7
  }
}
```

Use `offlineWer` as the primary accuracy gate unless a later G2 decision explicitly changes the benchmark target.
