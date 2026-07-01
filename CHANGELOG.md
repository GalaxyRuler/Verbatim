# Changelog

All notable changes to this project are documented here.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/). Versions track [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

<!-- Changes on main not yet in a tagged release go here -->

---

## [0.11.0] — 2026-07-01

### Added

- More Android on-device ASR engine packs, selectable in the Models tab: SenseVoice (multilingual zh/en/ja/ko/yue), Canary (en/es/de/fr), Moonshine (fast English), Parakeet TDT 0.6B v2 (max-accuracy English). New engine-kind architecture supporting offline-only, VAD-segmented, final-only sessions.

### Fixed

- History database now recovers instead of failing to launch when it encounters a database written by a newer app version.

### Internal (CI)

- Android e2e now serves the bundled frontend (no localhost dev-URL race); Windows installer-smoke is robust to NSIS async uninstall timing.

---

## [0.10.0] — 2026-06-30

### Added

- **Android on-device speech recognition.** A new on-device ASR engine (sherpa-onnx streaming with a Whisper final pass) drives floating-bubble dictation, replacing the dependency on the OS speech recognizer.
- **Android model packs.** A real Models tab to download, verify, install, and select on-device ASR packs (including Whisper base.en and tier-A packs).
- **Optional on-device LLM cleanup (Android).** Tidy dictated text locally with a small on-device model (LiteRT-LM Qwen2.5), off by default.
- **Localized elevated-window notice.** The "dictation blocked by an elevated window" notification is now localized across supported languages, with a Settings toggle to control it.

### Fixed

- **Transcription no longer crashes on CPUs without AVX-512.** The bundled Whisper/ggml is now built with a portable AVX2 baseline instead of `-march=native`, fixing an illegal-instruction crash (`0xc000001d`) on recent Intel consumer CPUs (for example Core Ultra / Meteor Lake) that affected dictation in CPU mode.
- **Whisper Vulkan GPU loading** is guarded against a startup crash and falls back to CPU when GPU inference is unavailable.
- **CLI/global dictation toggle** reliably forwards to the already-running app instead of intermittently starting a second instance (the single-instance plugin is now registered first).
- **Settings** no longer double-subscribe to model-state updates.

### Security

- Updated `anyhow` to 1.0.103 (RUSTSEC-2026-0190).

---

## [0.9.0] — 2026-06-19

### Added

- **Android.** First public Android release, published as a signed universal APK (plus a universal AAB for app-bundle distribution) alongside `SHA256SUMS.txt` on the [GitHub Releases page](https://github.com/GalaxyRuler/Verbatim/releases/latest). See the [installation guide](docs/guide/installation.md) for sideload steps. Desktop builds (Windows, macOS, Linux) are unchanged by this release.

---

## [0.8.8] — 2026

### Added

- Local snippets for expanding saved phrases into longer reusable text.
- Local post-processing provider presets for LM Studio, Ollama, and vLLM.
- Side-by-side development build support for local testing without replacing the published app.

### Fixed

- Transcript privacy hardening: debug logs now record transcript length, not transcript content.
- Settings debug output now reports counts and redacts provider API keys.
- Invalid settings recovery now backs up the raw settings object and preserves valid user fields when possible.
- Clipboard restoration now preserves bitmap-only image payloads on Windows where possible and reports restore failures.
- Snippet triggers now tolerate common STT separators inside multi-word triggers.
- Too-short or high-risk stopword snippet triggers are rejected.

---

## [0.8.7] — 2026

### Added

- Mic diagnostics for silence, dead input, and short no-speech captures.
- Pill states for recording, silence, transcription, processing, paste failure, copied recovery, dictionary learning, and microphone failure.

### Fixed

- Short silent shortcut taps no longer paste hallucinated text.
- No-speech and microphone issue pill text now uses high-contrast foreground color.
- Selected-microphone availability issues are surfaced instead of silently falling back.

---

## [0.8.6] — 2026

### Added

- Docked pill mode with compact idle state and expanded action state.
- Pill language chip and language picker workflow.
- Recovery actions for copying and pasting the last transcript.

### Fixed

- Docked pill hitbox now collapses to the visible idle pill size.
- Paste-last-transcript recovery now works from the pill.
- Verbatim waveform/dots mark is used consistently in the pill.

---

## [0.8.5] — 2026

### Added

- Conservative language guard for locked-language contradictions.
- Paste failure recovery that keeps the final transcript recoverable on the clipboard.
- Remote LLM egress guard so remote post-processing requires a configured API key before transcript text leaves the device.

### Fixed

- Translation is treated as opt-in behavior instead of an accidental post-processing side effect.
- Language guard is bypassed only for explicit translation requests supported by the selected model.

---

## [0.8.4] — 2026

### Added

- Verbatim rebrand across app identifiers, package names, binary names, docs, workflows, and release surfaces.
- Verbatim asset CDN paths for bundled and downloadable model resources.
- User-friendly release links and updater metadata for direct app updates.

### Changed

- Project is detached from the original upstream fork path for independent Verbatim development.
- GitHub Actions release matrix focuses on Windows, macOS Apple Silicon, and Ubuntu.

---

## [0.8.3] — 2025

### Added

- Intel Mac (x86_64) build instructions with dynamic ONNX Runtime linking via Homebrew
- Portuguese translation for portable update flow
- AWS Bedrock (Mantle) as a post-processing provider option
- Opt-in reasoning effort passthrough for local models (avoids thinking-mode latency)
- Cohere as a post-processing provider

### Fixed

- Overlay / pasting issue on Linux documented with workaround (Settings → Advanced → Overlay Position → None)
- GTK layer shell overlay initialization on KDE Plasma under Wayland
- `VERBATIM_NO_GTK_LAYER_SHELL` environment variable now parsed correctly as boolean
- Crash on older CPUs (pre-AVX2)
- Whisper model crash surface area reduced; paste errors now shown as UI toast
- German translation quality improvements

### Changed

- Nix: drop redundant `LD_LIBRARY_PATH` wrapper prefix; rely on `cargo-tauri.hook` standard phases
- GPU info query is now async (no startup block)
- `transcribe-rs` upgraded to 0.3.5 → 0.3.8

### Documentation

- `AGENTS.md` updated as the single source of truth for AI coding assistants
- GitHub workflow rules for AI coding assistants made actionable
- Linux startup troubleshooting section expanded

---

## [0.8.2]

### Added

- Parakeet V3 CPU-optimized speech recognition model with automatic language detection
- Italian translation
- Cohere integration as post-processing provider

### Fixed

- Crash on CPUs without AVX2 support
- Nix: remove onnxruntime overlay, use nixpkgs native package
- Preserve legacy portable marker during updates

### Changed

- VAD and model loading now run in parallel at startup

---

## [0.8.x] — Earlier 0.8 releases

### Highlights

- Tauri 2.x migration
- `transcribe-rs` library for unified Whisper + Parakeet inference
- GPU-accelerated Whisper on Metal (macOS), Vulkan (Windows/Linux), DirectML (Windows)
- Global shortcut handling rewrite with Verbatim Keys
- Tauri specta for type-safe command bindings
- Internationalization (i18next): en, de, es, fr, ja, zh, vi
- Transcription history with SQLite storage
- Post-processing pipeline (LLM providers: OpenAI, Anthropic, local models, Bedrock, Cohere)
- CLI remote control flags (`--toggle-transcription`, `--cancel`, `--start-hidden`)
- Unix signal support for Wayland hotkey daemons (`SIGUSR1`, `SIGUSR2`)
- Recording overlay with platform-specific positioning

---

## Upstream

This project is a fork of [Handy](https://github.com/cjpais/Handy) by [CJ Pais](https://github.com/cjpais). For the complete upstream changelog see the [upstream releases page](https://github.com/cjpais/Handy/releases).

[Unreleased]: https://github.com/GalaxyRuler/Verbatim/compare/v0.11.0...HEAD
[0.11.0]: https://github.com/GalaxyRuler/Verbatim/compare/v0.10.0...v0.11.0
[0.10.0]: https://github.com/GalaxyRuler/Verbatim/compare/v0.9.0...v0.10.0
[0.9.0]: https://github.com/GalaxyRuler/Verbatim/compare/v0.8.8...v0.9.0
[0.8.8]: https://github.com/GalaxyRuler/Verbatim/compare/v0.8.7...v0.8.8
[0.8.7]: https://github.com/GalaxyRuler/Verbatim/compare/v0.8.6...v0.8.7
[0.8.6]: https://github.com/GalaxyRuler/Verbatim/compare/v0.8.5...v0.8.6
[0.8.5]: https://github.com/GalaxyRuler/Verbatim/compare/v0.8.4...v0.8.5
[0.8.4]: https://github.com/GalaxyRuler/Verbatim/compare/v0.8.3...v0.8.4
[0.8.3]: https://github.com/GalaxyRuler/Verbatim/releases/tag/v0.8.3
[0.8.2]: https://github.com/GalaxyRuler/Verbatim/releases/tag/v0.8.2
