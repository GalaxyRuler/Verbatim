# Changelog

All notable changes to this project are documented here.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/). Versions track [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

<!-- Changes on main not yet in a tagged release go here -->

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
- `HANDY_NO_GTK_LAYER_SHELL` environment variable now parsed correctly as boolean
- Crash on older CPUs (pre-AVX2)
- Whisper model crash surface area reduced; paste errors now shown as UI toast
- German translation quality improvements

### Changed
- Nix: drop redundant `LD_LIBRARY_PATH` wrapper prefix; rely on `cargo-tauri.hook` standard phases
- GPU info query is now async (no startup block)
- `transcribe-rs` upgraded to 0.3.5 → 0.3.8

### Documentation
- `CLAUDE.md` and `AGENTS.md` unified into single source of truth for AI assistants
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
- Global shortcut handling rewrite with `handy-keys`
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

[Unreleased]: https://github.com/GalaxyRuler/Verbatim/compare/v0.8.3...HEAD
[0.8.3]: https://github.com/cjpais/Handy/releases/tag/v0.8.3
[0.8.2]: https://github.com/cjpais/Handy/releases/tag/v0.8.2
