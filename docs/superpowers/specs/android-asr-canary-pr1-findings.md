# Android ASR Canary PR1 Findings

Date: 2026-06-30
Branch: `codex/android-asr-canary-tierb`
Base: `main` at `e263ee1d`

## Decision

Canary PR1 is implemented and ready for maintainer audit plus Tab S9+ device verification. Stop here; Moonshine and Parakeet are not included.

The main checkpoint finding was language handling: a first AVD smoke with hardcoded `src_lang=en`, `tgt_lang=en` produced an English translation-like result for the German fixture. The final branch keeps `new_canary(paths)` as the default English constructor, but the Android session and `asr-wer` now call `new_canary_for_language(paths, lang)`, which sets `tgt_lang = src_lang` for `en`, `es`, `de`, or `fr`. Translation-to-English with `src_lang != tgt_lang` remains a future toggle and is not in this PR.

## Model

- Repo: `csukuangfj/sherpa-onnx-nemo-canary-180m-flash-en-es-de-fr-int8`
- Revision: `9077164e0d3dd1d5353743e89ceaa1d3a770838c`
- Pack id: `canary-180m-flash-en-es-de-fr`
- Layout: `canary/encoder.onnx`, `canary/decoder.onnx`, `canary/tokens.txt`, `silero_vad_v4.onnx`
- Manifest size: `207 MB`
- RAM gate: `minRamMb = 6144`

| File | Source file | Size | SHA-256 |
| --- | --- | ---: | --- |
| `canary/encoder.onnx` | `encoder.int8.onnx` | 132,678,643 | `7a75b4e2a5857a6dcc0819503bbe3fad66943db4a3ccf21d3f27c633667d303f` |
| `canary/decoder.onnx` | `decoder.int8.onnx` | 74,437,848 | `e41a2ab9c0c2fe81a1e8ade5a45fb02a74bc4db7d1f91b89a54a25e2cf79cba2` |
| `canary/tokens.txt` | `tokens.txt` | 53,555 | `2dae6fc7815f9640645e0c765522b278ee0cef49b482d91f6913e334628d3e77` |
| `silero_vad_v4.onnx` | `silero_vad_v4.onnx` | 1,807,522 | `a35ebf52fd3ce5f1469b2a36158dba761bc47b973ea3382b3186ca15b1f5af28` |

Note: `decoder.onnx` at the pinned HF revision returned the 15-byte text `Entry not found`; the actual pinned model artifact is `decoder.int8.onnx`, verified above and mapped into the pack as `canary/decoder.onnx`.

## TDD Evidence

| Check | RED | GREEN |
| --- | --- | --- |
| Canary path inference | `session_kind_uses_canary_when_canary_layout_exists` failed before `AsrEngineKind::Canary` and Canary paths existed. | `scripts/cargo-test-windows.ps1 session_kind_uses_canary_when_canary_layout_exists` passed. |
| Event shape | `asr_command_event_shape_matches_engine_kind` failed on non-exhaustive Canary match. | Event-shape test now asserts Canary and SenseVoice final-only, Zipformer/Whisper partials plus final. |
| Canary config | Android target test failed before Canary config helper existed. | `canary_config_uses_transcribe_mode_pnc_tokens_and_cpu` passes with encoder, decoder, tokens, CPU, PNC, default `en -> en`. |
| Source-language transcription | `canary_config_for_language_keeps_transcription_target_on_source_language` failed before language-aware Canary config existed. | Android target no-run test passes with `de-DE -> src_lang=de, tgt_lang=de`. |
| Kotlin required files | Canary layout test failed before `requiredFilesForPack` knew `canary/`. | `engineModelInstallationAcceptsCanaryLayoutWithoutStreamingFiles` passes. |
| `asr-wer` smoke helper | Android `cargo check --bin asr-wer --target x86_64-linux-android --features android-asr` failed on non-exhaustive Canary matches. | Same check passes; Canary is offline-only and uses the selected source language. |

## AVD Smoke

Target: `emulator-5554` (`sdk_gphone64_x86_64`)

Setup:
- Installed `src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk`.
- Copied the pack into `/data/user/0/com.galaxyruler.verbatim/models/android-asr/canary-180m-flash-en-es-de-fr`.
- Set `native_engine_model_id=canary-180m-flash-en-es-de-fr`, then force-stopped the app before smoke.
- Fixture: pinned `test_wavs/de.wav`, SHA-256 `36d3c4845b9808a1656a2a2e92d884590e2db94389e6fe559643291ae0cd3710`, resampled to 16 kHz PCM for the debug feed, SHA-256 `d0b6146019f40e1c64acece0b806151a58a8220fd32defc141976589df2f304c`.
- Command path: debug broadcast `com.galaxyruler.verbatim.action.DEBUG_ENGINE_WAV_SMOKE` with `wav_path=/data/user/0/com.galaxyruler.verbatim/files/de-16k.wav` and `lang=de`.

Log evidence:

```text
nativeAsrStart called lang=de modelDir=/data/user/0/com.galaxyruler.verbatim/models/android-asr/canary-180m-flash-en-es-de-fr debugWav=/data/user/0/com.galaxyruler.verbatim/files/de-16k.wav
nativeAsrStart returned true
nativeAsrStop called
onFinal callback len=37
nativeAsrStop returned true
```

Results:

| Assertion | Result |
| --- | --- |
| `nativeAsrStart` returned true | PASS |
| No `onPartial callback` | PASS |
| One final callback | PASS |
| Final text is German | PASS: `hat ein Ende . Nur die Wurst hat zwei` |
| Strict fatal crash grep | PASS: `NO_MATCH` for `FATAL EXCEPTION|SIGSEGV|Fatal signal|libc.*Fatal|Abort message` |

## Gates

| Gate | Result |
| --- | --- |
| `bun run check:translations` | PASS: 19 locales complete against 808 English keys |
| `bun run lint` | PASS |
| `bun run build` | PASS |
| `scripts/cargo-test-windows.ps1` | PASS: 534 tests |
| Android Canary config no-run target test | PASS |
| Android `asr-wer` x86_64 check | PASS |
| `bun run android:test:unit` | PASS |
| `bun run tauri android build --debug --target x86_64 --apk --features android-asr` | PASS |
| Android generated-file guard | PASS |
| `bun scripts/check-android-so-alignment.ts ...` with NDK LLVM on `PATH` | PASS: 12 files |
| `maestro --device emulator-5554 test maestro/` | PASS: `android-smoke` |

## PR Notes

- Human Written Description remains a PR-template TODO for the maintainer.
- Community Feedback remains a PR-template TODO.
- Device audit remains for Tab S9+: download Canary, auto-select, smoke with `de.wav`/`es.wav`/`fr.wav`, confirm final-only, no crash, and check Zipformer/Whisper partials regression.
