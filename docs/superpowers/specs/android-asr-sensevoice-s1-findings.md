# Android SenseVoice S1 Findings

Date: 2026-06-30
Branch: `codex/android-sensevoice-s1`

## Decision

S1 proves that the pinned SenseVoice model loads and produces zh/en/ja/ko/yue transcripts on both the x86_64 AVD and the arm64 Tab S9+ standalone harness. Proceed to S2 only after maintainer review of the evidence below.

One transcript-quality note: the arm64 Tab S9+ English run produced `50 pieces of code` where the x86_64 AVD produced `50 pieces of gold`.

## Model

- Repo: `csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17`
- Revision: `2365baeacb507f821a0c8120fcee3d484dba7a07`
- Layout: `sense_voice/model.onnx`, `sense_voice/tokens.txt`, `silero_vad_v4.onnx`
- Scratch model root: `C:\CodexScratch\verbatim-sensevoice-s1\model`
- Scratch corpus root: `C:\CodexScratch\verbatim-sensevoice-s1\corpus`

| File | Size | SHA-256 |
| --- | ---: | --- |
| `sense_voice/model.onnx` | 239,233,841 | `c71f0ce00bec95b07744e116345e33d8cbbe08cef896382cf907bf4b51a2cd51` |
| `sense_voice/tokens.txt` | 315,894 | `f449eb28dc567533d7fa59be34e2abca8784f771850c78a47fb731a31429a1dc` |
| `silero_vad_v4.onnx` | 1,807,522 | `a35ebf52fd3ce5f1469b2a36158dba761bc47b973ea3382b3186ca15b1f5af28` |

## Device Results

| Target | ABI | 16 KB clean | Loaded | zh | en | ja | ko | yue | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| AVD `emulator-5554` (`sdk_gphone64_x86_64`) | x86_64 | PASS | PASS | 开放时间早上9点至下午5点。 | The tribal chieftain called for the boy and presented him with 50 pieces of gold. | うちの中学は弁当制で持っていけない場合は50円の学校販売のパンを買う。 | 조 금만 생각 을 하 면서 살 면 훨씬 편할 거야. | 呢几个字都表达唔到我想讲嘅意思。 | Init 1380.3 ms; peak HWM 363,212 kB; MemAvailable before run 1,410,644 kB; strict crash grep `NO_MATCH`. |
| Tab S9+ `R52WA0H3ASM` (`SM-X810`, Android 16) | arm64-v8a | PASS | PASS | 开放时间早上9点至下午5点。 | The tribal chieftain called for the boy and presented him with 50 pieces of code. | うちの中学は弁当制で持っていけない場合は50円の学校販売のパンを買う。 | 조 금만 생각 을 하 면서 살 면 훨씬 편할 거야. | 呢几个字都表达唔到我想讲嘅意思。 | Init 1079.0 ms; peak HWM 350,464 kB; MemAvailable before run 4,052,956 kB; strict crash grep `NO_MATCH`; English differs from AVD/reference word `gold`. |

## Latency And RAM

| Target | Init ms | Language | Samples | Decode latency ms | VmRSS kB | VmHWM kB | Transcript |
| --- | ---: | --- | ---: | ---: | ---: | ---: | --- |
| AVD x86_64 | 1380.3 | zh | 89,472 | 80.6 | 359,180 | 360,244 | 开放时间早上9点至下午5点。 |
| AVD x86_64 | 1380.3 | en | 114,432 | 99.4 | 362,336 | 362,732 | The tribal chieftain called for the boy and presented him with 50 pieces of gold. |
| AVD x86_64 | 1380.3 | ja | 115,200 | 101.0 | 362,592 | 363,212 | うちの中学は弁当制で持っていけない場合は50円の学校販売のパンを買う。 |
| AVD x86_64 | 1380.3 | ko | 73,728 | 70.3 | 362,536 | 363,212 | 조 금만 생각 을 하 면서 살 면 훨씬 편할 거야. |
| AVD x86_64 | 1380.3 | yue | 82,368 | 77.6 | 360,696 | 363,212 | 呢几个字都表达唔到我想讲嘅意思。 |
| Tab S9+ arm64 | 1079.0 | zh | 89,472 | 183.9 | 346,352 | 346,720 | 开放时间早上9点至下午5点。 |
| Tab S9+ arm64 | 1079.0 | en | 114,432 | 235.1 | 349,676 | 350,100 | The tribal chieftain called for the boy and presented him with 50 pieces of code. |
| Tab S9+ arm64 | 1079.0 | ja | 115,200 | 240.7 | 349,888 | 350,464 | うちの中学は弁当制で持っていけない場合は50円の学校販売のパンを買う。 |
| Tab S9+ arm64 | 1079.0 | ko | 73,728 | 145.1 | 349,824 | 350,464 | 조 금만 생각 을 하 면서 살 면 훨씬 편할 거야. |
| Tab S9+ arm64 | 1079.0 | yue | 82,368 | 164.9 | 349,964 | 350,464 | 呢几个字都表达唔到我想讲嘅意思。 |

## Commands And Evidence

- Config test: `cargo test --manifest-path src-tauri\Cargo.toml sense_voice_config_uses_auto_language_itn_tokens_and_cpu --lib --features android-asr --target x86_64-linux-android --no-run`
- Existing Whisper compile path: `cargo test --manifest-path src-tauri\Cargo.toml offline_whisper_transcribes_fixture --lib --features android-asr --target x86_64-linux-android --no-run`
- Smoke build x86_64: `cargo build --manifest-path src-tauri\Cargo.toml --release --target x86_64-linux-android --features android-asr --bin asr-sensevoice-smoke`
- Smoke build arm64: `cargo build --manifest-path src-tauri\Cargo.toml --release --target aarch64-linux-android --features android-asr --bin asr-sensevoice-smoke`
- 16 KB guard x86_64: `bun scripts/check-android-so-alignment.ts "C:\CodexScratch\verbatim-android-asr-g0-official\android-x86_64-v1.13.3\lib"` -> `Android .so 16 KB alignment check passed for 2 file(s).`
- 16 KB guard arm64: `bun scripts/check-android-so-alignment.ts "C:\CodexScratch\verbatim-android-asr-g0-official\android-arm64-v1.13.3\lib"` -> `Android .so 16 KB alignment check passed for 2 file(s).`
- AVD run log: `C:\CodexScratch\verbatim-sensevoice-s1\artifacts\avd-x86_64-run-rebased.log`
- Tab S9+ run log: `C:\CodexScratch\verbatim-sensevoice-s1\artifacts\tab-s9p-arm64-run-rebased.log`

## Crash Scan

Strict crash grep used after each run:

```powershell
adb -s <serial> logcat -d | Select-String -Pattern 'FATAL|AndroidRuntime|SIGSEGV|Fatal signal'
```

Results:

| Target | Result |
| --- | --- |
| AVD `emulator-5554` | `NO_MATCH` |
| Tab S9+ `R52WA0H3ASM` | `NO_MATCH` |

The broader arm64 grep that included `asr-sensevoice|sherpa|onnx` matched only the `adbd` shell command lines and an `io_stats` line for the smoke process; it did not include `FATAL`, `AndroidRuntime`, `SIGSEGV`, or `Fatal signal`.

## Checkpoint

Stop here. No bubble/UI/model-pack integration is included in S1.
