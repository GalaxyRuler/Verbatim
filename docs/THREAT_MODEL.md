# Threat Model

This threat model focuses on the desktop app, local files, native insertion, provider credentials, and release/update trust.

## Assets

- Microphone audio.
- Raw and post-processed transcripts.
- Selected text used by transform actions.
- Clipboard contents before and during insertion.
- History database and recording files.
- Provider API keys.
- Model files and local-LLM artifacts.
- Release artifacts and updater metadata.

## Trust Boundaries

- Webview frontend to Rust backend through Tauri commands.
- Main window to overlay window through events.
- Rust backend to operating-system APIs for audio, clipboard, keyboard, accessibility, filesystem, credentials, and tray.
- Backend to local model files and downloaded artifacts.
- Backend to remote text providers when enabled.
- Installed app to GitHub release updater metadata and artifacts.
- External script insertion to user-configured local executables.

## Primary Risks

### Sensitive Text Side Effects

Text can be pasted, copied, stored, sent to a provider, or sent to an external script after the user cancels or changes focus.

Controls:

- Operation cancellation tokens.
- Target fingerprint checks before insertion when context is available.
- Clipboard ownership checks before restore.
- External script stdin instead of argv.

### Credential Exposure

Provider API keys could leak through settings, debug output, frontend state, logs, or screenshots.

Controls:

- OS credential store for API keys.
- Frontend receives stored-secret placeholders, not raw keys.
- Debug formatting redacts secret maps.
- Logs must not print provider request headers or keys.

### Webview Overreach

If the webview is compromised, broad filesystem, asset, or plugin permissions increase blast radius.

Controls:

- CSP is explicit rather than null.
- Asset protocol and fs scopes are narrowed to recordings paths.
- Overlay window has core-only capability.
- CI validates security config.

### Model And Dependency Integrity

Model downloads and dependencies can become tampered, yanked, vulnerable, or unexpectedly sourced.

Controls:

- Official model catalog entries require HTTPS and SHA-256.
- Rust Git dependencies are pinned by commit `rev`.
- Cargo-deny checks advisories, licenses, bans, and sources.
- Release artifacts include checksums and manifest metadata.

### Update And Release Trust

Users can install incomplete, unsigned, or unverifiable artifacts if release gates are weak.

Controls:

- Release finalizer requires expected desktop artifacts, updater signatures, `latest.json`, checksums, and release manifest.
- Release body states signing status.
- Signing and release checklists document manual verification steps.

## Non-Goals

- Protecting against a fully compromised operating system account.
- Preventing a user-approved external script from reading the text it is explicitly given.
- Claiming remote text providers retain no data; provider behavior is outside Verbatim's control and must be disclosed.
- Guaranteeing identical behavior on unsupported release targets.

## Review Checklist

- Does the change add a new place where audio, transcript text, selected text, clipboard content, API keys, or model files can move?
- Does it cross a trust boundary listed above?
- Does cancellation still block late side effects?
- Does public UI copy distinguish local-only behavior from networked provider behavior?
- Does the change need a narrower Tauri permission, fs scope, or CSP source?
- Does the change require release, signing, SBOM, or updater documentation updates?
