# Architecture

Verbatim is a Tauri 2 desktop application with a Rust backend and React frontend. The main design goal is local-first dictation with explicit boundaries around networked text providers, native insertion, local files, and updater behavior.

## Process Shape

- `src-tauri/src/lib.rs` builds the Tauri app, registers commands/events, initializes managers, and owns startup wiring.
- `src/main.tsx` renders the main settings window.
- `src/overlay/` renders the recording overlay window.
- The app enforces single-instance behavior. Secondary launches send CLI arguments to the running instance and exit.

## Backend Domains

- `managers/audio.rs`: recording lifecycle, device selection, and audio manager state.
- `managers/model.rs` and `managers/model_catalog.rs`: model catalog, downloads, local paths, checksums, and model state.
- `managers/transcription.rs`: local speech-to-text execution and engine lifetime.
- `actions.rs`: high-level dictation flow from recording stop through transcription, post-processing, history, and insertion.
- `runtime_settings.rs`: converts persisted settings into runtime policy decisions for post-processing and transforms.
- `clipboard.rs`, `insertion.rs`, and `dictation_transaction.rs`: insertion method choice, clipboard handling, target checks, and side-effect outcomes.
- `settings.rs`: persisted settings shape, defaults, migrations, and store writes.
- `credentials.rs`: provider API-key storage through the operating-system credential store.
- `commands/`: Tauri command handlers that expose safe backend operations to the frontend.
- `shortcut/`: global shortcuts, command handlers for settings updates, and remote-control entry points.

## Frontend Domains

- `src/App.tsx`: main window shell, section routing, and cross-window events.
- `src/components/settings/`: settings screens and controls.
- `src/components/model-selector/`: model management UI.
- `src/components/overlay/` and `src/overlay/`: recording feedback UI.
- `src/stores/`: Zustand stores for settings/model state.
- `src/bindings.ts`: generated Tauri command and event bindings.
- `src/i18n/`: i18next setup and locale files.

## Command And Event Flow

Frontend-to-backend calls use generated bindings from `src/bindings.ts`. Backend-to-frontend notifications use Tauri events. The common flow is:

1. A user changes a setting or presses a shortcut.
2. The frontend calls a generated command, or the backend shortcut handler starts an action directly.
3. Backend code reads settings, applies runtime policy, and mutates native state.
4. Backend emits state events for model, overlay, history, errors, or recovery feedback.
5. Frontend stores update their local view from command results and events.

Adding or changing backend commands/types requires regenerating `src/bindings.ts` from a debug build path before the frontend can rely on the new shape.

## Dictation Flow

1. Global shortcut starts recording.
2. Audio manager records and VAD filters input.
3. Stop/cancel creates or terminates an operation token.
4. Transcription manager runs the selected local engine.
5. Runtime policy decides whether remote or local text post-processing is allowed.
6. History and recording retention policy decide what to persist.
7. Insertion code verifies the target when context is available, then pastes, types, copies, or runs an external script.
8. Overlay and history events report the outcome.

Cancellation must be checked before every late side effect: provider calls, WAV writes, history writes, queued insertions, paste, copy, and retry actions.

## Provider Architecture

Speech providers are still being extracted from the current manager-oriented implementation. The target provider design is described in `docs/adr/0001-engine-provider-architecture.md`.

Text post-processing and transform providers are intentionally separate from speech providers because they operate on already-transcribed text and may send user text to local or remote HTTP endpoints.

## Settings And Secrets

Settings are stored with the Tauri store plugin. Settings may include preferences, model IDs, prompt templates, and non-secret provider metadata.

Provider API keys must not be stored as plaintext settings. API keys are stored through `credentials.rs`; public settings responses return only a stored-secret placeholder.

## Release And Update Flow

Release builds are produced by GitHub Actions. The release finalizer requires desktop installers, updater artifacts, signatures, `latest.json`, `SHA256SUMS.txt`, and `RELEASE_MANIFEST.json`.

Installed desktop builds use the Tauri updater. Portable installs are expected to use manual updates.
