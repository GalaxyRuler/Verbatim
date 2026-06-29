# Android G4 on-device LLM post-processing

Status: implementation spike in progress on `codex/android-asr-g4-llm-postproc`.
Scope: Android only. The feature is optional, off by default, model-download driven, and raw ASR text remains recoverable through native history.

## Decision

Use **LiteRT-LM Android 0.13.1** with the **Qwen2.5 0.5B Instruct LiteRT `.task` pack** (`Qwen2.5-0.5B-Instruct_multi-prefill-seq_q8_ekv1280.task`) for the first G4 implementation.

Runtime and model:

- Runtime: `com.google.ai.edge.litertlm:litertlm-android:0.13.1`, Kotlin API.
- Model pack id: `g4-qwen2_5-0_5b-litert-q8`.
- Model URL: `https://huggingface.co/litert-community/Qwen2.5-0.5B-Instruct/resolve/6c237a59eedeb06a821b21f0a59b03d346ac8bc3/Qwen2.5-0.5B-Instruct_multi-prefill-seq_q8_ekv1280.task`.
- Model SHA-256: `e608953f169aeb1bd7b9155fec2559825e08453fc209b84eda3a781ed0452fd2`.
- Model size: `546,660,344` bytes, surfaced as `522 MB`.
- License: Apache-2.0.
- Backend for G4a: CPU first, 4 threads, deterministic sampler. GPU/NPU stays a manifest/runtime extension after device evidence.

Integration note: the Android project currently compiles with Kotlin `1.9.25`, while LiteRT-LM `0.13.1` publishes Kotlin `2.2/2.3` metadata. The app packages LiteRT-LM as a runtime-only Android dependency, excludes Kotlin transitive artifacts, and calls the narrow API surface through a local reflection adapter. This avoids a project-wide Kotlin upgrade for an off-by-default Android-only feature while still packaging the latest Google runtime.

Why this over the alternatives:

- LiteRT-LM is Google's current Android LLM path. Google documents MediaPipe LLM Inference as legacy/maintenance and points new Android work at LiteRT-LM, while LiteRT-LM provides a Kotlin API that avoids adding a custom JNI runtime layer.
- Qwen2.5 0.5B is ungated and Apache-2.0. Gemma 3 1B LiteRT has an excellent footprint, but the official LiteRT Community repo is Gemma-licensed and Hugging Face gated; the app should not require a user token for the first Android cleanup pack.
- llama.cpp remains viable and more controllable, but it would add a second native runtime, JNI ownership, GGUF packaging, and another 16 KB page-size surface. That is heavier than the narrow cleanup task needs.

Primary sources checked:

- LiteRT-LM overview and Android Kotlin API: https://developers.google.com/edge/litert-lm/overview
- MediaPipe LLM Inference note that new Android integrations should use LiteRT-LM: https://ai.google.dev/edge/mediapipe/solutions/genai/llm_inference/android
- Qwen LiteRT Community model metadata: https://huggingface.co/litert-community/Qwen2.5-0.5B-Instruct
- Gemma 3 1B LiteRT Community model metadata: https://huggingface.co/litert-community/Gemma3-1B-IT
- llama.cpp Android docs: https://github.com/ggml-org/llama.cpp/blob/master/docs/android.md
- Android 16 KB page-size requirement: https://developer.android.com/guide/practices/page-sizes

## Gate and device tier

The settings toggle is visible but disabled unless the native Android gate passes:

- `arm64-v8a` ABI is available.
- Total device RAM is at least `8192 MB`.
- SoC is a known high-end mobile tier, currently Snapdragon `SM8550+`, Tensor G3/G4/G5, or MediaTek Dimensity 9300/9400-class identifiers. Devices with at least `12288 MB` RAM can pass when the SoC identifier is missing but the ABI is arm64.

The threshold is deliberately conservative because G4 loads an ASR engine plus an LLM cleanup runtime in the same user workflow. It is not a quality default; it only controls whether the off-by-default toggle can be enabled.

Device snapshot gathered from the attached physical-tier device:

| Field | Value |
| --- | --- |
| ADB serial | `R52WA0H3ASM` |
| Model | `SM-X810` |
| Hardware | `qcom` |
| SoC model | `SM8550` |
| 64-bit ABI | `arm64-v8a` |
| `/proc/meminfo` total | `11,454,088 kB` |
| `/proc/meminfo` available at sampling | `4,833,088 kB` |
| Gate result | supported |

## Size and alignment evidence

LiteRT-LM Android `0.13.1` was downloaded from Google Maven. AAR size: `21,937,043` bytes.

The existing repo guard was run against the extracted AAR native libraries:

```powershell
$env:ANDROID_NDK_HOME='C:\Users\Admin\AppData\Local\Android\Sdk\ndk\28.2.13676358'
bun scripts/check-android-so-alignment.ts "$env:TEMP\verbatim-litertlm-aar\unpacked\jni"
```

Result: `Android .so 16 KB alignment check passed for 6 file(s).`

| ABI | Library | Size bytes | 16 KB guard |
| --- | --- | ---: | --- |
| arm64-v8a | `libLiteRt.so` | `5,064,144` | pass |
| arm64-v8a | `libLiteRtClGlAccelerator.so` | `2,778,136` | pass |
| arm64-v8a | `liblitertlm_jni.so` | `14,882,984` | pass |
| x86_64 | `libLiteRt.so` | `6,997,664` | pass |
| x86_64 | `libLiteRtClGlAccelerator.so` | `3,466,448` | pass |
| x86_64 | `liblitertlm_jni.so` | `18,047,168` | pass |

Model comparison:

| Candidate | Runtime | Artifact | Size bytes | License/access | Decision |
| --- | --- | --- | ---: | --- | --- |
| Qwen2.5 0.5B Instruct q8 | LiteRT-LM | `.task` | `546,660,344` | Apache-2.0, ungated | selected |
| Gemma 3 1B int4 | LiteRT-LM | `.task` | `554,661,243` | Gemma terms, gated | defer until app distribution path is clear |
| Qwen2.5 0.5B Q4_K_M | llama.cpp | `.gguf` | about `469 MB` in desktop catalog | Apache-2.0, ungated | defer; second runtime and JNI surface |

## Runtime contract

Cleanup runs only after ASR final text exists and before insertion. On any unsupported device, missing model, disabled toggle, exception, timeout, blank output, excessive expansion, or likely language/script loss, Verbatim inserts the raw ASR text through the existing native formatter path.

Prompt contract:

- Fix punctuation, capitalization, spacing, and obvious dictation artifacts.
- Do not translate.
- Do not add facts, greetings, signoffs, explanations, or new content.
- Preserve languages, scripts, names, numbers, URLs, emails, and mixed-language text.
- Return only the cleaned transcript.

The native history entry keeps raw ASR in `transcription_text`; cleaned text is stored as `post_processed_text` only when inserted text differs.

## G4a checkpoint

Completed locally:

- Runtime and model choice grounded from current primary sources.
- LiteRT-LM AAR downloaded and 16 KB-guarded.
- Attached device RAM/SoC sampled and gate documented.

Still requires the branch APK with the LLM model installed to complete the end-to-end load prompt on device:

- Download/install the `g4-qwen2_5-0_5b-litert-q8` pack.
- Toggle cleanup on.
- Run a debug cleanup prompt and record peak RSS/logcat timing.
- Dictate with cleanup on and off to confirm cleaned insertion versus raw insertion.

## Debug cleanup smoke runner

Debug APKs register `DebugInsertProbeReceiver`, which can trigger the cleanup smoke without hand-patching the service. Release builds do not include this receiver.

```sh
adb broadcast -a com.galaxyruler.verbatim.action.DEBUG_LLM_CLEANUP_SMOKE \
  -n com.galaxyruler.verbatim/.DebugInsertProbeReceiver \
  --es raw_text "hello comma this is a test" --es model_path <app-readable .task path>
```

Gotchas:

- The app UID cannot read model files passed directly from `/data/local/tmp`. Copy the `.task` file into the app data directory with `run-as` first, then pass that app-readable path as `model_path`.
- The first cold load of the 546 MB Qwen `.task` model takes time; wait for the debug log before treating the smoke as stuck.
- The success/diagnostic log line is `FloatingBubble: debug LLM cleanup smoke rawLen=.. cleanedLen=..`. `cleanedLen=0` means the cleanup output was validation-rejected or the fallback path returned no cleaned text.
