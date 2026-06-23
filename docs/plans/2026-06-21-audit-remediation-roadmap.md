# Audit Remediation Roadmap Implementation Plan

**Goal:** Turn the static desktop audit into a sequenced remediation program that improves Verbatim's release trust, privacy posture, desktop reliability, and contributor confidence without over-claiming unsupported platforms.

**Architecture:** Treat this as a multi-track hardening program, not a single feature branch. The first tracks establish truthful docs and release gates, then fix user-safety invariants in cancellation/insertion/clipboard handling, then add native CI, startup recovery, credential storage, platform readiness, and signing/provenance. Long-term work expands only after test capacity exists.

**Tech Stack:** Tauri 2, Rust, React, TypeScript, Bun, Vite, tauri-specta bindings, GitHub Actions, Playwright, platform credential stores, native desktop test harnesses.

---

## Current Scope And Assumptions

- Scope is the desktop application and repository readiness for Verbatim.
- The audit is a static review; findings marked **Needs manual verification** must be reproduced or disproved before shipping invasive fixes.
- Feature freeze remains in effect. Reliability, security, privacy, docs, diagnostics, and release gates are allowed hardening work; new end-user features should be framed as risk reduction.
- Public-hygiene rules apply. Do not add local operator artifacts, local machine paths, or private notes to public release branches.
- For broad Rust validation on Windows, keep environment/toolchain failures separate from application regressions.

## Agreement And Disagreement With The Audit

### I Agree Strongly

- **Unsigned Windows/macOS releases are a real blocker.** SmartScreen and Gatekeeper warnings create a normal-user trust failure and make spoofed artifacts harder for users to reason about.
  - Pros of fixing: normal install trust, enterprise viability, clearer provenance.
  - Cons: certificate cost, secret management, notarization friction, release workflow complexity.
- **Native desktop CI and packaged smoke tests are underweight.** Browser tests with mocked Tauri APIs cannot protect shortcut, tray, permission, audio, updater, and insertion regressions.
  - Pros: catches the exact failures users experience.
  - Cons: slower, flakier, requires OS runners and careful fixture design.
- **Cancellation, insertion target safety, and clipboard ownership are top safety invariants.** Dictation text is sensitive, and "Cancel" must mean no later paste or provider request.
  - Pros: directly reduces privacy and wrong-target harm.
  - Cons: requires cross-cutting changes through coordinator, model pipeline, post-processing, history, clipboard, and UI.
- **Privacy claims were too broad.** Local transcription is not the same as no network, no retained data, or no cloud text flow.
  - Pros: improves user trust and reduces misleading claims.
  - Cons: less marketable wording and more nuanced UX copy.
- **Startup panics and dead coordinator state are product reliability defects.** A desktop utility must recover or explain itself.
  - Pros: fewer silent failures and support cases.
  - Cons: requires a bootstrap error model and UI path before the app is fully initialized.
- **Tauri CSP, asset protocol, and capability scopes need hardening.** Desktop webview compromise should not imply broad app-data reach.
  - Pros: reduces blast radius.
  - Cons: easy to break model/audio/history asset loading if changed without integration tests.
- **Secret storage should leave the settings store.** Redacted debug formatting is not at-rest protection.
  - Pros: protects API keys in backups and local file compromise.
  - Cons: Linux credential-store availability is inconsistent and migration needs careful rollback behavior.
- **Supply-chain policy is too loose with branch-based Git dependencies.**
  - Pros: reproducibility, reviewability, easier incident response.
  - Cons: slower updates and more dependency-management ceremony.

### I Agree, With Reframing

- **"No Critical finding" is reasonable from static evidence.** I would not label anything Critical without a demonstrated exploit or confirmed sensitive-data loss.
  - Reframe: cancellation/insertion privacy invariants should still be treated as P0 product safety work.
- **"Best next move" should not be only native smoke/signing.** I agree release gates matter, but I would not sign and promote the product before fixing cancellation/insertion and privacy retention gaps.
  - Better sequence: truthful docs and release completeness first, then cancellation/insertion, then native release gates, then signing.
- **Desktop and Android release-page separation is valid but lower risk.** It should be fixed because it reduces user confusion, but it should not displace safety or release-integrity work.
- **Accessibility work is necessary but should start with automation and keyboard coverage before full assistive-technology certification.** NVDA, VoiceOver, and Orca gates are valuable, but they need a maintained manual matrix.
- **Model hardware recommendations should be evidence-backed.** Do not publish speculative performance tables without local benchmarks and user-safe caveats.

### I Disagree Or Would Defer

- **Do not gate every PR immediately on full packaged-app smoke tests.** Start by making release candidates require native smoke tests. Add PR-level native compile/unit checks first, then expand PR gates once flake is controlled.
- **Do not add AppImage/Flatpak/RPM because users ask for them before Linux helper behavior is stable.** More formats without shortcut/insertion test coverage increases support risk.
- **Do not implement encrypted local history before zero-retention and keychain storage.** Encryption is useful, but users first need reliable "do not keep this" controls and clean deletion semantics.
- **Do not refactor `App.tsx` or settings domains before safety invariants are protected.** The refactor is real technical debt, but it is lower urgency than cancellation, insertion, startup recovery, secrets, and release gates.

## Program Order

1. **Foundation already started:** privacy/security docs, visible Diagnostics, bug template, release asset checks, checksum manifest.
2. **P0 safety:** cancellation, destination-safe insertion, clipboard transaction ownership.
3. **P0/P1 release assurance:** native desktop CI, packaged smoke, update tests, artifact manifest.
4. **P1 privacy/security:** zero-retention, keychain migration, Tauri CSP/capabilities.
5. **P1 reliability:** startup recovery, coordinator supervisor, GPU/provider fallback.
6. **P1 onboarding/platform readiness:** first-success loop and Linux helper diagnostics.
7. **P2 supply-chain/docs/accessibility/performance:** SBOM, dependency policy, architecture/threat docs, accessibility gates, benchmarks.
8. **Long-term:** signing maturity, encrypted history, reproducible builds, broader platforms only when test capacity exists.

## Track 0: Baseline And Issue Triage

**Purpose:** Convert the audit into tracked work and prevent duplicate or conflicting fixes.

**Files:**

- Modify: `.github/ISSUE_TEMPLATE/bug_report.md`
- Modify: `.github/ISSUE_TEMPLATE/config.yml`
- Create: `.github/ISSUE_TEMPLATE/security_hardening.md` only if maintainers want public tracking for non-sensitive hardening.
- Create: `docs/audits/2026-06-21-desktop-remediation-tracker.md`

- [x] Create a public tracker mapping every audit finding to an issue, owner label, and release milestone.
- [x] Mark findings as `confirmed`, `needs-manual-verification`, `partially-fixed`, or `deferred`.
- [x] Split issue labels into `priority:p0`, `priority:p1`, `priority:p2`, `area:release`, `area:privacy`, `area:insertion`, `area:testing`, `area:tauri`, `area:onboarding`, `area:accessibility`, `area:dependencies`.
- [x] Keep security exploit details out of public issues; reference `SECURITY.md` for private reports.

**Progress 2026-06-22:**

- Added `docs/audits/2026-06-21-desktop-remediation-tracker.md` as the public audit tracker.
- The tracker maps each public-safe audit finding to status, priority, owner/area labels, milestone, public issue placeholder, current evidence, and next action.
- The tracker keeps sensitive exploit, transcript, recording, credential, full app-data, and unredacted-log details out of public issue flow and points private reports to `SECURITY.md`.

**Verification:**

```bash
bunx prettier --check .github/ISSUE_TEMPLATE docs/audits
bun scripts/check-public-hygiene.ts --all
```

## Track 1: Privacy, Data Retention, And User Truthfulness

**Purpose:** Align claims, UI, docs, and defaults with actual storage/network behavior.

**Files:**

- Modify: `README.md`
- Modify: `CONTRIBUTING.md`
- Modify: `docs/PRIVACY.md`
- Modify: `src/i18n/locales/en/translation.json`
- Modify: `src/components/settings/post-processing/PostProcessingSettings.tsx`
- Modify: `src/components/settings/PostProcessingSettingsApi/index.tsx`
- Modify: `src/components/settings/advanced/AdvancedSettings.tsx`
- Modify: `src/components/settings/history/HistorySettings.tsx`
- Modify: `src-tauri/src/settings.rs`
- Modify: `src-tauri/src/managers/history.rs`
- Test: `src-tauri/src/managers/history.rs` tests
- Test: add frontend tests after frontend unit-test harness exists in Track 3

### Task 1.1: Finish Privacy Disclosure

- [x] Ensure every user-facing "offline" claim says "local audio transcription" rather than "no network."
- [x] Add an in-app disclosure next to remote post-processing activation: remote providers receive transcript text or selected text.
- [x] Add an in-app storage disclosure in History settings: history can contain raw transcript, post-processed text, transform text, and linked WAV files.
- [x] Add docs for uninstall data retention and manual app-data deletion.

**Progress 2026-06-22:**

- The post-processing toggle now discloses that API mode sends transcript text to the configured provider.
- The post-processing engine state now discloses that API mode can send transcript or selected text to the selected provider and model.
- The history limit setting now discloses that history can include raw transcripts, refined text, transform text, and linked WAV files.
- `docs/PRIVACY.md` documents app-data locations, uninstall behavior, and manual deletion of transcripts, recordings, models, settings, logs, and API keys.
- The README privacy badge alt text now says local-first privacy rather than offline; the remaining no-network scan hits are deliberate explanatory warnings in `docs/PRIVACY.md` and this roadmap.
- Verification completed: `bun run check:translations`, `bun run lint`, `bun run build`, and `git diff --check`.

**Verification:**

```bash
rg -n "completely offline|no cloud|no network|without sending" README.md docs src
bun run check:translations
bun run lint
bun run build
```

Expected: `rg` has no misleading absolute privacy claims outside deliberate explanatory text.

### Task 1.2: Zero-Retention And Clear-All Controls

- [x] Add explicit settings for `history_enabled` and `recordings_enabled`.
- [x] Make `history_limit = 0` and `recording_retention_period = preserve_limit` select all existing unsaved dictation history for deletion.
- [x] Add `clear_history` and `clear_recordings` commands.
- [x] Add History UI buttons: clear all history, clear recordings, and explain saved-entry behavior.
- [x] Ensure transform-only entries do not evict dictation entries unless clear-all is selected.
- [x] Test clean profile defaults and zero-retention behavior.

**Progress 2026-06-22:**

- Confirmed the existing count-cleanup path treats `history_limit = 0` as deleting all unsaved recording-backed dictation entries while preserving transform-only entries from count eviction.
- Fixed the retention dropdown display so a zero history limit is shown as `Keep latest 0` instead of falling back to `Keep latest 5`.
- Added `clear_history` and `clear_recordings` commands and regenerated `src/bindings.ts`.
- Added History UI controls for Clear recordings and Clear all history, with confirmation prompts and helper copy explaining that Clear recordings preserves saved entries and transform-only text while Clear all history removes saved and unsaved entries.
- `clear_recordings` deletes unsaved recording-backed rows and their WAV files, then reloads history so preserved saved/transform rows remain visible. `clear_history` deletes all rows and removes associated recording files.
- Added `history_enabled` and `recordings_enabled` settings, Advanced > History toggles, generated bindings, and persistence gates so history off stores no future transcript rows while recordings off stores text-only history without WAV files.
- Text-only history rows hide replay/retry controls, while copy, save, delete, and dictionary correction remain available.
- `clear_recordings` now targets file-backed unsaved dictation rows only; general retention still applies to unsaved dictation rows, including text-only rows, so recording storage off does not create unbounded history.
- Added compile-only history tests for zero-limit deletion candidates, saved-entry preservation at zero limit, transform exclusion from count eviction, and clear-recordings candidate selection.
- Tightened clean-profile default tests for enabled history/recording storage, the default history limit, and the default preserve-limit retention mode.
- Native packaged smoke now reports and enforces clean-profile retention defaults from its isolated profile: history enabled, recordings enabled, `history_limit = 5`, `recording_retention_period = preserve_limit`, zero history rows, and zero recording files.
- Native packaged smoke now runs the production dictation storage-policy seam for default, recordings-disabled, history-disabled, and private-session cases, failing if any case would persist history/WAV contrary to policy.
- Remaining scope: packaged/runtime zero-retention drills that exercise actual audio/model dictation flows.
- Compile-only verification: `cargo fmt --manifest-path src-tauri/Cargo.toml`, `cargo check --manifest-path src-tauri/Cargo.toml --no-default-features --quiet`, `cargo build --manifest-path src-tauri/Cargo.toml --no-default-features --quiet`, debug binary binding export from `src-tauri`, `cargo test --manifest-path src-tauri/Cargo.toml history --lib --no-default-features --no-run --quiet`, `bun run check:translations`, `bun run lint`, `bun run build`, and `git diff --check`. Runtime verification: pending native-backend CI job execution on GitHub Actions.

**Verification:**

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml history --lib
bun run check:translations
bun run lint
bun run build
```

### Task 1.3: Private Session

- [x] Add a runtime-only private-session flag that disables history commit, WAV persistence, post-paste learning, and post-processing.
- [x] Surface private-session state in the overlay and settings.
- [x] Ensure private-session state is never persisted as a surprising default toggle.
- [x] Add tests proving private-session operations leave no history row and no WAV file at the storage-policy seam.

**Progress 2026-06-22:**

- Added process-local `PrivateSessionState`, `get_private_session_status`, and `set_private_session_enabled`; state is managed at app setup and is not part of persisted `AppSettings`.
- Private session now suppresses future dictation history rows, WAV writes, requested post-processing, transform actions, history retry, and post-paste dictionary auto-learning.
- Added a runtime Private Session toggle under Advanced > History and an overlay indicator driven by the same backend event.
- Added compile-only tests for the in-memory state and dictation storage policy. Native packaged smoke now also exercises the production storage-policy seam for private-session suppression. Remaining scope: packaged/runtime drills proving a real private-session dictation creates no history row and no WAV file.

**Pros:**

- Directly answers the audit's local retention concern.
- Gives sensitive-use users a simple mental model.

**Cons:**

- Cross-cuts history, recordings, dictionary learning, and provider flow.
- Bad implementation could create false confidence, so tests must cover every output path.

## Track 2: Cancellation, Insertion, And Clipboard Safety

**Purpose:** Guarantee that sensitive text is never inserted, sent, or retained after cancellation or target change.

**Files:**

- Modify: `src-tauri/src/transcription_coordinator.rs`
- Modify: `src-tauri/src/managers/transcription.rs`
- Modify: `src-tauri/src/actions.rs`
- Modify: `src-tauri/src/clipboard.rs`
- Modify: `src-tauri/src/platform/*` insertion/focus modules
- Modify: `src-tauri/src/commands/transcript.rs`
- Modify: `src-tauri/src/commands/transform.rs`
- Modify: `src/App.tsx`
- Modify: `src/overlay/*`
- Test: Rust unit tests near coordinator/actions/clipboard/insertion
- Test: native smoke tests in Track 3

### Task 2.1: Operation-Scoped Cancellation Token

- [x] Create an operation ID and cancellation token when recording starts.
- [x] Pass the token through stop-recording, model load/inference, post-processing, history commit, clipboard mutation, insertion, and auto-submit.
- [x] On cancel, set token state to `Cancelled` and emit a UI event that explicitly ends the operation.
- [x] Check cancellation immediately before any network request, history insert/update, clipboard write, paste/typing, external script invocation, or auto-submit.
- [x] If cancellation occurs during provider inference that cannot be aborted, discard the result and block all side effects.

**Progress 2026-06-22 (External Scripts):**

- Added operation-scoped cancellation state and token propagation through dictation stop, provider request, post-processing boundaries, history save, and classic/adaptive insertion scheduling.
- Canceled operations now discard late transcription results, avoid failed-history retry rows, delete canceled WAV/history artifacts where possible, and block queued main-thread insertion if cancellation arrives before paste.
- Live dictation now passes the operation token into transcription post-processing and checks cancellation immediately before managed-local, structured-provider, and legacy-provider post-processing requests.
- Live dictation now passes the operation token through adaptive/classic insertion into clipboard insertion, blocking cancellation-sensitive clipboard writes, paste shortcuts, direct typing, external-script invocation, auto-submit, and copy-to-clipboard. If cancellation arrives after Verbatim writes a temporary clipboard payload but before the paste shortcut, it restores the prior clipboard when the payload is still owned.
- Transform actions now begin an operation token, check cancellation before transform provider requests, selected-text replacement, recovery clipboard copy, and transform-history save.
- History retry now begins an operation token, uses cancellable transcription, checks cancellation before retry post-processing and history update, and passes the token into retry post-processing.
- Live dictation and history retry now stop waiting behind model-load completion when cancellation arrives before or during the wait. The current native provider load API is still synchronous once entered, so this protects operation side effects without claiming mid-call native preemption.
- Remaining scope: full default-feature native verification.
- Compile-only verification: `cargo fmt --manifest-path src-tauri/Cargo.toml`, `cargo test --manifest-path src-tauri/Cargo.toml "operation_cancellation|post_processing|storage_policy|private_session" --lib --no-default-features --no-run --quiet`, `cargo test --manifest-path src-tauri/Cargo.toml "cancellation_guard|operation_cancellation|external_script_invocation" --lib --no-default-features --no-run --quiet`, `cargo test --manifest-path src-tauri/Cargo.toml "cancelled_transform|cancelled_retry|operation_cancellation" --lib --no-default-features --no-run --quiet`, `cargo test --manifest-path src-tauri/Cargo.toml "no_engine_transcription_still_honors_cancellation_first|operation_cancellation" --lib --no-default-features --no-run --quiet`, and `cargo check --manifest-path src-tauri/Cargo.toml --no-default-features --quiet`. Runtime verification: pending native-backend CI job execution on GitHub Actions.
- Verification blocked: default-feature `cargo test --manifest-path src-tauri/Cargo.toml "model_load_wait" --lib --no-run --quiet` previously exited before crate compilation in the native `whisper-rs-sys` CMake/MSBuild build on this Windows machine. A retry with `CARGO_TARGET_DIR=C:\t-verbatim`, `CMAKE_GENERATOR=Ninja`, `TrackFileAccess=false`, `CMAKE_BUILD_PARALLEL_LEVEL=1`, and `CARGO_BUILD_JOBS=1` did not fail immediately but still timed out after 15 minutes while building native dependencies, so full default-feature native verification remains unproven locally.

**Verification:**

```bash
cargo test --manifest-path src-tauri/Cargo.toml cancellation --lib
cargo test --manifest-path src-tauri/Cargo.toml coordinator --lib
bun run build
```

### Task 2.2: Destination-Safe Insertion Transaction

- [x] Define one `InsertionTransaction` abstraction for clipboard, direct typing, auto-submit, transform replacement, and retry paste.
- [x] Capture destination fingerprint before recording or before transform action starts.
- [x] Verify destination fingerprint immediately before insertion.
- [x] If destination changed, block insertion, copy text only if configured/recoverable, and show "Copy" or "Paste Here" recovery.
- [x] Ensure standard dictation and adaptive path use the same transaction.
- [x] External scripts must receive text via stdin, not command-line arguments.

**Progress 2026-06-22 (Classic Destination Safety):**

- `PasteMethod::ExternalScript` now spawns the configured script with piped stdin/stdout/stderr and writes dictated text to stdin instead of adding it to argv.
- Added a pure invocation-contract test proving the text payload is absent from external-script arguments.
- Compile-only verification: `cargo fmt --manifest-path src-tauri/Cargo.toml`, `cargo test --manifest-path src-tauri/Cargo.toml external_script_invocation --lib --no-default-features --no-run --quiet`, `cargo check --manifest-path src-tauri/Cargo.toml --no-default-features --quiet`, and `git diff --check`. Runtime verification: pending native-backend CI job execution on GitHub Actions.

**Progress 2026-06-22:**

- Classic dictation now captures a target context when context awareness is enabled, even when adaptive profiles are disabled.
- Classic insertion now rechecks the foreground target before paste when an original target fingerprint exists, and blocks insertion with a not-attempted receipt if the destination changed.
- Added pure tests for classic target-change insertion outcomes, classic verification gates, and target-context capture without adaptive profiles.
- Added a shared `InsertionTransaction` wrapper over the pure insertion resolver, and routed classic dictation, adaptive dictation, and paste-last transcript through it.
- External script insertion now keeps dictated text on stdin, suppresses stdout/stderr echoing in failure text, polls operation cancellation while the child process runs, and kills the child when cancellation arrives.
- Transform replacement already recaptures the selected target and selection before mutating; its exact replacement path now receives the operation cancellation token so direct typing, clipboard paste, and external-script replacement can abort before late side effects.
- Remaining scope: broader native target-change smoke tests.
- Compile-only verification: `cargo fmt --manifest-path src-tauri/Cargo.toml`, `cargo test --manifest-path src-tauri/Cargo.toml "classic_target|target_context_capture" --lib --no-default-features --no-run --quiet`, `cargo test --manifest-path src-tauri/Cargo.toml "external_script_invocation|external_script_failure_message|cancellation_guard" --lib --no-default-features --no-run --quiet`, `cargo test --manifest-path src-tauri/Cargo.toml "insertion_transaction|paste_last_failure" --lib --no-default-features --no-run --quiet`, `cargo check --manifest-path src-tauri/Cargo.toml --no-default-features --quiet`, and `git diff --check`. Runtime verification: pending native-backend CI job execution on GitHub Actions.

**Progress 2026-06-22 (Transform Transaction):**

- Added an explicit transform-replacement insertion attempt to the shared transaction resolver, with auto-learning disabled.
- Routed transform selected-text replacement through `InsertionTransaction`, while preserving the transform-specific recovery-copy event and history status.
- Transform replacement now reports success/failure through the same insertion receipt shape used by clipboard, direct typing, external scripts, auto-submit-adjacent insertion, paste-last, adaptive dictation, and classic dictation.
- Compile-only verification: `cargo fmt --manifest-path src-tauri/Cargo.toml`, `cargo test --manifest-path src-tauri/Cargo.toml "transform_replacement" --lib --no-default-features --no-run --quiet`, and `cargo check --manifest-path src-tauri/Cargo.toml --no-default-features --quiet`. Runtime verification: pending native-backend CI job execution on GitHub Actions.

**Progress 2026-06-22 (Recovery UX):**

- Paste failures now emit structured recovery metadata for ordinary paste failure, target-change blocks, and language-guard blocks.
- Target-change blocks no longer show the misleading generic copied-to-clipboard message; they show an insertion-blocked recovery with a "Paste here" action.
- Ordinary paste failures still keep the existing recovery-copy behavior and "Copy again" action.
- Language-guard blocks continue to use their dedicated "Paste anyway" recovery toast and no longer duplicate the generic paste-failure toast.
- Remaining scope: packaged native focus-switch smoke tests.
- Compile-only verification: `cargo fmt --manifest-path src-tauri/Cargo.toml`, `cargo test --manifest-path src-tauri/Cargo.toml "target_changed_is_not_attempted|adaptive_ready_failure_keeps_recovery_copy|classic_guard_block" --lib --no-default-features --no-run --quiet`, `cargo check --manifest-path src-tauri/Cargo.toml --no-default-features --quiet`, `bun run build`, `bun run check:translations`, `bun run lint`, `$env:PLAYWRIGHT_USE_SYSTEM_CHROME='1'; bunx playwright test tests/app.spec.ts -g 'target-changed|language guard paste-error|paste failure toast' --project=chromium`, and `$env:PLAYWRIGHT_USE_SYSTEM_CHROME='1'; bunx playwright test tests/app.spec.ts -g 'paste failure toast|target-changed paste recovery|language guard paste-error' --project=chromium --timeout=60000`. Runtime verification: pending native-backend CI job execution on GitHub Actions.

**Verification:**

```bash
cargo test --manifest-path src-tauri/Cargo.toml insertion --lib
cargo test --manifest-path src-tauri/Cargo.toml transform --lib
bun run lint
bun run build
```

### Task 2.3: Clipboard Ownership

- [x] Add a unique clipboard transaction marker or platform sequence check where available.
- [x] Restore previous clipboard only if the clipboard still contains Verbatim's payload or marker.
- [x] If another process changed the clipboard, leave it untouched and log a redacted diagnostic.
- [x] Replace fixed paste-delay assumptions with an ownership-aware timeout plus user-configurable fallback.
- [x] Add tests for text, empty clipboard, image/file clipboard, rich-text where supported, and concurrent clipboard mutation.

**Progress 2026-06-22:**

- Clipboard paste restore is now guarded by ownership: Verbatim restores the prior clipboard only when the clipboard still contains the exact temporary payload it wrote.
- If the clipboard changed before restore, Verbatim leaves it untouched and logs a redacted warning.
- Before sending the paste shortcut, Verbatim now polls for readback ownership of its temporary payload using the existing user-configured paste delay as the timeout, with a small minimum polling window.
- Added focused predicate/timeout tests for exact payload ownership, changed text, unreadable/non-text clipboard states, and zero-delay timeout behavior.
- Clipboard ownership now records the Windows clipboard sequence number after Verbatim writes its temporary payload and requires the sequence to match before paste/restore when the platform exposes it. Platforms without a sequence number continue to use exact text readback.
- Added pure ownership tests for matching sequence markers, same-text concurrent mutation with a changed sequence, no-text/unreadable clipboard states, and no-sequence fallback.
- Remaining scope: native clipboard race coverage for image/file/rich-text formats in packaged smoke.
- Compile-only verification: `cargo fmt --manifest-path src-tauri/Cargo.toml`, `cargo test --manifest-path src-tauri/Cargo.toml "clipboard_restore_requires_own_payload|external_script_invocation" --lib --no-default-features --no-run --quiet`, `cargo check --manifest-path src-tauri/Cargo.toml --no-default-features --quiet`, and `git diff --check`. Runtime verification: pending native-backend CI job execution on GitHub Actions.

**Progress 2026-06-22 (Sequence Marker):**

- Added a Windows clipboard sequence-number marker to the temporary paste payload ownership check, so another process writing the same text after Verbatim's clipboard write is treated as a mutation and does not trigger restore.
- Added pure tests for exact text ownership, no/unreadable text, matching sequence, changed same-text sequence, and sequence-unavailable fallback.
- Compile-only verification: `cargo fmt --manifest-path src-tauri/Cargo.toml`, `cargo test --manifest-path src-tauri/Cargo.toml "clipboard_restore" --lib --no-default-features --no-run --quiet`, and `cargo check --manifest-path src-tauri/Cargo.toml --no-default-features --quiet`. Runtime verification: pending native-backend CI job execution on GitHub Actions.

**Progress 2026-06-22 (Clipboard Format Tests):**

- Added explicit pure tests for changed-text concurrent mutation with a matching sequence marker, empty text clipboard rejection, plain text snapshot capture, image fallback after native restore failure, Windows file-drop format restoration, and registered rich-text format restoration.
- The new file/rich-text coverage exercises the Windows native-memory format restore seam (`CF_HDROP` and registered format IDs) without reading real user clipboard content.
- Packaged native smoke now reports and validates a clipboard safety drill for same-text sequence mutation, changed-text mutation, and exact-text no-sequence fallback. The drill does not read or write the real clipboard, but it keeps the packaged smoke contract tied to the same ownership predicate that decides whether Verbatim may restore the clipboard.
- Remaining scope: packaged native race coverage for real image/file/rich-text clipboards in controlled desktop sessions.
- Compile-only verification: `cargo fmt --manifest-path src-tauri/Cargo.toml`, `cargo test --manifest-path src-tauri/Cargo.toml clipboard --lib --no-default-features --no-run --quiet`, `cargo check --manifest-path src-tauri/Cargo.toml --no-default-features --quiet`, and `git diff --check`. Runtime verification: pending native-backend CI job execution on GitHub Actions.
- Runtime note: `cargo test --manifest-path src-tauri/Cargo.toml clipboard --lib --no-default-features --quiet` compiled but the test binary exited at startup with Windows `STATUS_ENTRYPOINT_NOT_FOUND` before running assertions on this host, so the proof here is compile-only plus the existing roadmap's packaged-smoke follow-up.

**Pros:**

- Strongly reduces wrong-target and clipboard-race privacy risk.

**Cons:**

- Platform clipboard APIs differ; complete ownership checks may need platform-specific fallbacks.

## Track 3: Native CI, Packaged Smoke, And Release Gates

**Purpose:** Stop shipping desktop regressions that mocked browser tests cannot see.

**Files:**

- Modify: `.github/workflows/test.yml`
- Modify: `.github/workflows/build.yml`
- Modify: `.github/workflows/release.yml`
- Modify: `.github/workflows/desktop-parity.yml`
- Create: `.github/workflows/native-backend.yml`
- Create: `.github/workflows/native-smoke.yml`
- Create: `tests/native-smoke/`
- Create: `scripts/native-smoke/`
- Create: `docs/QA.md`

### Task 3.1: PR-Level Native Compile And Unit Tests

- [x] Add Windows x64, macOS ARM64, and Ubuntu x64 jobs that compile production desktop modules.
- [ ] Mark the native backend jobs required in branch protection.
- [x] Remove or retire test substitutions that hide the production transcription coordinator from required checks once the native lane is stable.
- [x] Keep platform-specific environment setup explicit, including Vulkan/DirectML/macOS SDK constraints.
- [x] Run Rust unit tests on all three platforms.

**Progress 2026-06-22:**

- Added `.github/workflows/native-backend.yml` as a PR/push/manual lane for default-feature Rust backend `cargo check` and `cargo test` on Windows x64, macOS ARM64, and Ubuntu x64.
- The native lane does not copy `transcription_mock.rs` over the production transcription manager, so it exercises the real default-feature provider/coordinator compile path.
- Removed the legacy `.github/workflows/test.yml` source-copy substitution that overwrote `src/managers/transcription.rs` with `transcription_mock.rs`; the lightweight no-default test lane now uses the existing feature-gated mock path without mutating source files.
- Windows setup uses Ninja, a short target directory, `TrackFileAccess=false`, and Vulkan SDK installation, reusing the repo's Windows cargo wrapper scripts and test manifest.
- Ubuntu setup installs WebKitGTK, appindicator, ALSA, OpenBLAS, X11, gtk-layer-shell, Vulkan, Mesa Vulkan drivers, and `glslang-tools`; macOS targets `aarch64-apple-darwin`.
- Documented the required branch-protection status contexts in `docs/QA.md` and `docs/RELEASE_CHECKLIST.md`: `Windows x64 production backend`, `macOS ARM64 production backend`, and `Ubuntu x64 production backend`.
- Live GitHub API check on 2026-06-22 returned `Branch not protected` for `main`, so the branch-protection checkbox remains open until the repository setting is approved and applied after the workflow lands on the default branch.
- Added `bun run check:branch-protection`, backed by `scripts/check-branch-protection.ts`, to verify that `main` requires the Windows, macOS, and Ubuntu native backend contexts once the workflow has landed and repository-owner approval is available.
- Live GitHub API check on 2026-06-23 still returned `Branch not protected` for `main`, so the setting remains unapplied in GitHub.
- Verification completed for the branch-protection verifier: `bunx tsc --noEmit --pretty false --skipLibCheck --module NodeNext --moduleResolution NodeNext --target ES2022 --types node scripts/check-branch-protection.ts`, `bunx prettier --check scripts/check-branch-protection.ts package.json docs/QA.md docs/plans/2026-06-21-audit-remediation-roadmap.md`, and `bun run check:branch-protection` failing with `gh: Branch not protected (HTTP 404)`.
- Remaining scope: mark the new jobs as required in repository branch protection after explicit repo-setting approval; packaged smoke tests still belong to Task 3.2.

**Verification:**

```bash
bun run check:backend:windows
bun run test:backend:windows
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
```

### Task 3.2: Release-Candidate Packaged Smoke

- [x] Build packages on each release platform.
- [x] Launch packaged app headlessly or with a controlled desktop session.
- [x] Smoke: first launch, settings load, single-instance behavior, tray initialization, updater initialization, close-to-tray, clean quit.
- [x] Use a fake or tiny local model path for smoke tests; do not require paid or external provider calls.
- [x] Store logs and screenshots as workflow artifacts on failure.

**Progress 2026-06-22 (Native Packaged Smoke Harness):**

- Added `.github/workflows/native-smoke.yml` as an unsigned packaged smoke lane for Windows x64, macOS ARM64, and Ubuntu x64.
- Added `scripts/native-smoke/run-packaged-smoke.ts`, exposed as `bun run smoke:native`, to locate the packaged executable, launch it with an isolated profile, exercise a duplicate single-instance launch, collect stdout/stderr, validate app-written smoke status, write `native-smoke-summary.json`, and capture best-effort screenshots.
- Added the `VERBATIM_SMOKE_EXIT_AFTER_MS` and `VERBATIM_SMOKE_STATUS_PATH` runtime hooks so packaged CI can prove startup, settings load, main-window creation, tray initialization, updater plugin registration, single-instance plugin registration, close-to-tray handler registration, debug-mode override, and clean process exit without manual window control.
- Added the smoke-only `VERBATIM_SMOKE_MODEL_FIXTURE=1` hook so packaged CI creates a tiny local `verbatim-smoke-model.bin` in the app's actual model directory before model discovery. The smoke status now verifies that the selected model is configured, custom, downloaded, and has no remote URL, without requiring a paid provider or external model download.
- Made `.github/workflows/native-smoke.yml` reusable and added a `release-smoke` gate so release finalization waits for the packaged smoke workflow to pass after platform packages are built.
- Added `scripts/native-smoke/run-installer-smoke.ts`, exposed as `bun run smoke:installer`, and wired it into the native smoke workflow. Installer smoke installs the produced Windows NSIS artifact into a temporary normal install with shortcuts disabled, copies the macOS app out of the produced DMG, or installs the produced Ubuntu `.deb`, then runs the packaged smoke runner against the installed app path.
- Installer smoke cleanup exercises the generated Windows uninstaller and Ubuntu package removal path, failing if the installed executable remains. The Windows lane also seeds app-data markers in temporary `APPDATA`/`LOCALAPPDATA` sandboxes and verifies both default silent uninstall preservation and explicit `/DELETEAPPDATA` removal.
- The Windows NSIS uninstaller now accepts `/DELETEAPPDATA` for automated smoke coverage of the same app-data deletion branch used by the interactive checkbox.
- Added `docs/QA.md` and `tests/native-smoke/README.md` to document the current packaged-smoke scope and the remaining controlled audio/paste fixtures.
- The smoke lane currently uses `--no-tray` for deterministic CI startup. Tray construction, updater registration, single-instance registration/duplicate launch, close-to-tray handler registration, clean smoke exit, settings load/version metadata, and local model selection are now status-checked without remote calls, but visible tray interaction, updater UI, N-1 to N updater application, fake/tiny model transcription, virtual audio, controlled paste targets, focus-switch blocking, and real clipboard race drills remain open.
- Compile-only verification: `bun run smoke:native -- --help`, `bun run smoke:installer -- --help`, `bunx tsc --noEmit --pretty false --skipLibCheck --module NodeNext --moduleResolution NodeNext --target ES2022 --types node scripts/native-smoke/run-packaged-smoke.ts scripts/native-smoke/run-installer-smoke.ts`, `cargo test --manifest-path src-tauri/Cargo.toml native_smoke_model_fixture --lib --no-default-features --no-run --quiet`, `cargo check --manifest-path src-tauri/Cargo.toml --no-default-features --quiet`, `cd src-tauri; cargo fmt -- --check`, Prettier checks for the workflow, script, docs, and `git diff --check`. Runtime verification: pending native-backend CI job execution on GitHub Actions.
- Verification blocked: runtime execution of `cargo test --manifest-path src-tauri/Cargo.toml native_smoke_model_fixture --lib --no-default-features --quiet` still exits before the test body on this Windows host with `STATUS_ENTRYPOINT_NOT_FOUND`.

### Task 3.3: Virtual Audio And Controlled Paste Targets

- [x] Add a deterministic WAV fixture for smoke tests.
- [ ] Add an OS virtual audio input path for packaged transcription smoke tests.
- [x] Add controlled native paste targets: Notepad or plain textarea on Windows, TextEdit-like target on macOS, GTK/text field target on Linux.
- [ ] Test focus switch during inference and verify insertion is blocked.
- [ ] Test clipboard mutation during paste and verify Verbatim does not overwrite user clipboard changes.

**Pros:**

- Converts the audit's manual-only warnings into repeatable release evidence.

**Cons:**

- Native desktop automation is flaky unless the scope stays small and artifacts are excellent.

**Progress 2026-06-22 (Audio Fixture):**

- Added deterministic native-smoke audio fixture generation via `VERBATIM_SMOKE_AUDIO_FIXTURE_PATH`. The packaged app writes a two-second 16 kHz mono WAV through the app's own WAV utility, verifies sample count, reads it back, and reports the result in smoke status JSON.
- The smoke runner now fails if the fixture path is missing, the WAV file was not created, the sample count is not 32,000, or app-side WAV verification failed.
- Packaged native smoke now also runs an insertion-safety drill for adaptive and classic target-changed insertions. The runner fails if either path reaches the paste callback, marks insertion attempted, verifies the target, or reports anything other than `target changed before insertion`.
- Packaged native smoke now validates the clipboard safety drill described in Track 2.3, so target-change and clipboard-mutation safety predicates are both present in smoke status before real desktop-target automation is added.
- Added `scripts/native-smoke/controlled-desktop-targets.ts` and `bun run smoke:desktop-targets` to launch controlled OS text targets: Notepad on Windows, TextEdit on macOS, and `gedit`/`mousepad`/`xterm` on Linux. The helper can focus the target and, only when explicitly allowed in isolated CI, type/save a synthetic text marker and write/read a synthetic clipboard marker without reading user clipboard contents.
- `bun run smoke:native` now accepts `--desktop-target-drill`, `--require-desktop-target`, `--allow-text-entry`, and `--allow-clipboard-write`. The native smoke workflow exposes a matching manual/reusable `desktop-target-drill` input and installs Linux `xterm`/`xclip` helpers so release-candidate smoke can require the controlled desktop target drill without making ordinary PR smoke depend on GUI target availability.
- Hardened the controlled-target helper so it writes `controlled-desktop-targets.json` even when launch or clipboard mutation fails, records clipboard-drill failures instead of losing the artifact, and closes launched targets through a cleanup path.
- Added `--smoke-microphone <name>` / `VERBATIM_SMOKE_SELECTED_MICROPHONE` so virtual-audio smoke lanes can force a named microphone in the packaged app's isolated profile before `AudioRecordingManager` initializes. The smoke status now reports the selected microphone and the runner fails if it does not match.
- Added `scripts/native-smoke/virtual-audio-input.ts` and `bun run smoke:virtual-audio` to record virtual-input provisioning evidence. On Linux it can create a session-scoped PulseAudio/PipeWire source named `verbatim_smoke_source` through `pactl load-module`; on Windows/macOS it records the preinstalled virtual device name that should be passed to `--smoke-microphone`.
- Wired the native smoke workflow with opt-in `virtual-audio-preflight` and `virtual-audio-device-name` inputs. Linux installs PulseAudio tools, provisions `verbatim_smoke_source`, extracts `smoke_microphone_arg` from `virtual-audio-input.json`, and passes it to `bun run smoke:native -- --smoke-microphone`; Windows/macOS fail early if the input is enabled without an explicit preinstalled virtual device name.
- Added virtual-audio cleanup evidence: `bun run smoke:virtual-audio -- --cleanup-from ...` unloads the Linux PulseAudio modules from `virtual-audio-input.json` and writes `virtual-audio-cleanup.json`; the workflow runs this in an `always()` cleanup step. The workflow also exports `VERBATIM_SMOKE_SELECTED_MICROPHONE` so installer smoke uses the same provisioned input.
- Added virtual-audio playback evidence: `bun run smoke:virtual-audio -- --input-from ... --play-fixture ...` plays the app-generated deterministic WAV into the Linux virtual sink and writes `virtual-audio-playback.json`; the workflow runs it after packaged smoke using `audio_fixture_path` from `first-launch.status.json`.
- Added an explicit full app-driven insertion race evidence contract to `bun run smoke:native`: `--app-insertion-drills <path>` validates `app-insertion-drills.json`, and `--require-app-insertion-drills` fails release smoke if the full app-driven focus-switch and clipboard-mutation cases are missing. This does not treat controlled-target preflight as completion; it prevents future release candidates from accidentally passing without real app-driven evidence.
- Added `scripts/native-smoke/check-artifacts.ts`, exposed as `bun run check:native-smoke-artifacts`, to verify retained native-smoke release evidence before reviewers treat workflow artifacts as proof. The checker validates summary/status/log/screenshot artifacts by default, with opt-in strict gates for installer, controlled desktop target, virtual audio, and app-driven insertion race evidence.
- Wired `.github/workflows/native-smoke.yml` to run `bun run check:native-smoke-artifacts -- --dir native-smoke-artifacts --require-installer` before artifact upload, adding `--require-desktop-target` and `--require-virtual-audio` when those workflow inputs are enabled.
- Wired `.github/workflows/release.yml` to expose native-smoke strict-lane inputs and pass them into the reusable native-smoke workflow: `native_smoke_desktop_target_drill`, `native_smoke_virtual_audio_preflight`, and `native_smoke_virtual_audio_device_name`. Release dispatch now fails early if virtual-audio smoke is requested without the Windows/macOS device name.
- Hardened `bun run check:native-smoke-artifacts -- --require-installer` so installer evidence must contain a valid nested `installer/packaged-smoke/native-smoke-summary.json` and `installer/packaged-smoke/first-launch.status.json`, not just files with those names.
- Remaining scope: run real packaged inference against the provisioned virtual input, then wire the controlled paste/focus/clipboard targets into full Verbatim insertion smoke so the app performs the paste, focus switch, and clipboard mutation against real desktop targets.
- Verification completed for the release strict-smoke input wiring: `bunx prettier --check .github/workflows/release.yml docs/RELEASE_CHECKLIST.md docs/QA.md docs/plans/2026-06-21-audit-remediation-roadmap.md`, `rg -n "native_smoke_desktop_target_drill|native_smoke_virtual_audio_preflight|native_smoke_virtual_audio_device_name|desktop-target-drill:|virtual-audio-preflight:|virtual-audio-device-name:" .github/workflows/release.yml docs/RELEASE_CHECKLIST.md docs/QA.md docs/plans/2026-06-21-audit-remediation-roadmap.md`, and `bun run check:native-smoke-artifacts -- --help`.
- Verification completed for the retained-artifact gate: `bunx tsc --noEmit --pretty false --skipLibCheck --module NodeNext --moduleResolution NodeNext --target ES2022 --types node scripts/native-smoke/check-artifacts.ts`, `bunx prettier --check .github/workflows/native-smoke.yml scripts/native-smoke/check-artifacts.ts docs/QA.md tests/native-smoke/README.md docs/plans/2026-06-21-audit-remediation-roadmap.md package.json`, `bun run check:native-smoke-artifacts -- --help`, an expected missing-artifact failure from `bun run check:native-smoke-artifacts`, a strict synthetic artifact pass with `--require-installer --require-desktop-target --require-virtual-audio --require-app-insertion-drills`, a synthetic nested installer packaged-smoke failure rejected through `--require-installer`, and a synthetic nested installer packaged-smoke pass accepted through `--require-installer --require-platform win32`.
- Verification completed for the app-driven race evidence contract: `bunx tsc --noEmit --pretty false --skipLibCheck --module NodeNext --moduleResolution NodeNext --target ES2022 --types node scripts/native-smoke/run-packaged-smoke.ts`, `bunx prettier --check scripts/native-smoke/run-packaged-smoke.ts docs/QA.md tests/native-smoke/README.md docs/plans/2026-06-21-audit-remediation-roadmap.md`, and `bun run smoke:native -- --help`.
- Compile-only verification: `cargo test --manifest-path src-tauri/Cargo.toml "native_smoke_audio_fixture|native_smoke_failed_status" --lib --no-default-features --no-run --quiet`, `cargo check --manifest-path src-tauri/Cargo.toml --no-default-features --quiet`, `bunx tsc --noEmit --pretty false --skipLibCheck --module NodeNext --moduleResolution NodeNext --target ES2022 --types node scripts/native-smoke/run-packaged-smoke.ts`, `bun run smoke:native -- --help`, `bunx tsc --noEmit --pretty false --skipLibCheck --module NodeNext --moduleResolution NodeNext --target ES2022 --types node scripts/native-smoke/run-packaged-smoke.ts scripts/native-smoke/run-installer-smoke.ts scripts/native-smoke/controlled-desktop-targets.ts`, `bunx prettier --check .github/workflows/native-smoke.yml scripts/native-smoke/run-packaged-smoke.ts scripts/native-smoke/controlled-desktop-targets.ts tests/native-smoke/README.md package.json`, and `bun run smoke:desktop-targets -- --help`. Runtime verification: pending native-backend CI job execution on GitHub Actions.

## Track 4: Release Integrity, Signing, And Provenance

**Purpose:** Make release artifacts complete, verifiable, signed, and understandable.

**Files:**

- Modify: `.github/workflows/release.yml`
- Modify: `.github/workflows/build.yml`
- Modify: `docs/UPDATER_RELEASES.md`
- Modify: `README.md`
- Create: `docs/RELEASE_CHECKLIST.md`
- Create: `docs/SIGNING.md`

### Task 4.1: Complete Artifact Manifest

- [x] Keep the release finalizer requiring Windows EXE/MSI, macOS DMG, Linux DEB, updater archives, `.sig` files, `latest.json`, and `SHA256SUMS.txt`.
- [x] Expand manifest to include file size, SHA-256, updater platform key, updater signature presence, signing identity, and provenance/SBOM links.
- [x] Fail release if any expected artifact is absent or duplicated.
- [x] Separate desktop and Android asset sections in release body.

### Task 4.2: Windows Signing

- [ ] Select Authenticode provider and certificate process.
- [ ] Sign EXE, MSI, uninstaller, and app binaries as appropriate.
- [ ] Timestamp every signature.
- [x] Verify signatures in CI with `Get-AuthenticodeSignature` when `sign-binaries` is true.
- [x] Document signing identity and verification steps.

### Task 4.3: macOS Signing And Notarization

- [ ] Import Developer ID certificate in CI.
- [x] Review hardened runtime and entitlements.
- [ ] Sign app bundle and nested components.
- [ ] Notarize and staple DMG/app.
- [x] Verify codesign, Gatekeeper assessment, and stapling in CI when `sign-binaries` is true.
- [ ] Verify Gatekeeper behavior on clean macOS.

**Progress 2026-06-22:**

- Release finalizer now rejects duplicate asset names, requires expected desktop installers, verifies `latest.json` platform entries, requires updater assets, and requires matching `.sig` assets for updater URLs.
- Release finalizer publishes `SHA256SUMS.txt` and `RELEASE_MANIFEST.json`; the JSON manifest records file size, SHA-256, content type, download URL, updater platform key, updater signature presence, signing status, and provenance/SBOM fields.
- Release body now separates desktop downloads from Android downloads and links both verification manifests.
- Signed releases now require a public `signing_identity_label` workflow input; `RELEASE_MANIFEST.json` and generated release notes use that label instead of an opaque "configured in logs" placeholder.
- Release finalization now depends on the reusable packaged native smoke workflow so draft publication is gated by packaged startup/resource smoke on all release platforms.
- Added `docs/RELEASE_CHECKLIST.md` and `docs/SIGNING.md`.
- Signed builds now fail early when platform signing prerequisites are missing. Windows signed builds verify produced `.exe`/`.msi` artifacts with `Get-AuthenticodeSignature` and require timestamps. macOS signed builds verify strict `codesign`, Gatekeeper assessment, and stapling on produced `.app`/`.dmg` artifacts.
- `bun run check:tauri-security` now enforces the reviewed macOS hardened-runtime setting, minimum-system-version setting, entitlements file wiring, required microphone/audio-input entitlements, and absence of high-risk debug/code-loading entitlements.
- Installer smoke now launches the app from the produced NSIS/DMG/DEB artifact on each native-smoke platform before release finalization can complete, verifies Windows uninstaller plus Ubuntu package-removal cleanup for the smoke install, statically checks the Windows NSIS delete-app-data option exists, and verifies Windows app-data preservation/removal behavior in temporary sandboxes.
- The release checklist now requires reviewers to retain native-smoke artifacts and inspect `native-smoke-summary.json`, first-launch status, installer logs, controlled-target evidence, and virtual-audio input/playback/cleanup evidence when those opt-in lanes are enabled.
- Added `bun run check:release-evidence` and `bun run check:signed-release-evidence`, backed by `scripts/check-release-evidence.ts`, to validate downloaded `RELEASE_MANIFEST.json`, `SHA256SUMS.txt`, `latest.json`, updater signature coverage, signing identity fields, SBOM links, and provenance links before publishing or blessing a release.
- Added `bun run check:attested-release-evidence` and `bun run check:signed-attested-release-evidence`, plus the combined readiness flag `--require-attestations`, to require retained `gh attestation verify` status JSON for every packaged desktop asset before an attested release is blessed.
- Added `scripts/check-updater-smoke-evidence.ts`, exposed as `bun run check:updater-smoke-evidence` and `bun run check:updater-smoke-release-evidence`, to validate retained N-1 to N updater smoke JSON before release approval. The release gate requires evidence for `windows-x86_64`, `darwin-aarch64`, and `linux-x86_64`.
- Added `scripts/check-install-smoke-evidence.ts`, exposed as `bun run check:install-smoke-evidence`, `bun run check:install-smoke-release-evidence`, and `bun run check:install-smoke-signed-release-evidence`, to validate retained clean-machine install/uninstall smoke JSON before release approval.
- Added `scripts/check-release-readiness-evidence.ts`, exposed as `bun run check:release-readiness-evidence`, to run the retained release asset, native-smoke, install-smoke, updater-smoke, accessibility-smoke, optional attestation, optional benchmark, and optional branch-protection gates from one command. The combined check now requires native-smoke evidence for `win32`, `darwin`, and `linux`.
- Added `bun run check:release-gate-scripts` and wired it into `code-quality` so the Node-style release, benchmark, branch-protection, and native-smoke gate scripts are typechecked on PRs without adding Bun global typings for Bun-specific helper scripts.
- Verification completed for the combined release-readiness evidence verifier: `bunx tsc --noEmit --pretty false --skipLibCheck --module NodeNext --moduleResolution NodeNext --target ES2022 --types node scripts/check-release-readiness-evidence.ts`, `bun run check:release-readiness-evidence -- --help`, `bun run check:release-readiness-evidence` failing cleanly when evidence is absent, `bun run check:release-readiness-evidence -- --require-branch-protection --native-smoke-dir native-smoke-artifacts` reaching the branch-protection gate and failing with the current expected `Branch not protected (HTTP 404)` state, and a disposable complete bundle passing through release asset, win32/darwin/linux native-smoke, install-smoke, updater-smoke, and accessibility-smoke gates.
- Verification completed for the install-smoke evidence verifier: `bunx tsc --noEmit --pretty false --skipLibCheck --module NodeNext --moduleResolution NodeNext --target ES2022 --types node scripts/check-install-smoke-evidence.ts`, `bunx prettier --check scripts/check-install-smoke-evidence.ts package.json docs/RELEASE_CHECKLIST.md docs/SIGNING.md docs/UPDATER_RELEASES.md docs/plans/2026-06-21-audit-remediation-roadmap.md`, `bun run check:install-smoke-evidence -- --help`, an expected missing-evidence failure from `bun run check:install-smoke-evidence`, and a strict synthetic Windows/macOS/Linux signed pass through `bun run check:install-smoke-signed-release-evidence -- --dir <temp-install-smoke-evidence>`.
- Verification completed for the release-gate script typecheck: `bun run check:release-gate-scripts`, `bunx prettier --check package.json .github/workflows/code-quality.yml docs/plans/2026-06-21-audit-remediation-roadmap.md`, and `rg -n "check:release-gate-scripts|Typecheck release gate scripts|release-gate-scripts" package.json .github/workflows/code-quality.yml docs/plans/2026-06-21-audit-remediation-roadmap.md`.
- Verification completed for the release-evidence verifier: `bunx tsc --noEmit --pretty false --skipLibCheck --module NodeNext --moduleResolution NodeNext --target ES2022 --types node scripts/check-release-evidence.ts`, `bunx prettier --check scripts/check-release-evidence.ts package.json docs/RELEASE_CHECKLIST.md docs/SIGNING.md docs/plans/2026-06-21-audit-remediation-roadmap.md`, `bun run check:release-evidence -- --help`, `bun run check:release-evidence` failing cleanly when release evidence is absent, and `bun run check:signed-release-evidence -- --dir <temp-synthetic-release-evidence>` passing against a disposable signed manifest/checksum/latest bundle.
- Verification completed for the updater-smoke evidence verifier: `bunx tsc --noEmit --pretty false --skipLibCheck --module NodeNext --moduleResolution NodeNext --target ES2022 --types node scripts/check-updater-smoke-evidence.ts`, `bunx prettier --check scripts/check-updater-smoke-evidence.ts package.json docs/UPDATER_RELEASES.md docs/RELEASE_CHECKLIST.md docs/plans/2026-06-21-audit-remediation-roadmap.md`, `bun run check:updater-smoke-evidence -- --help`, an expected missing-evidence failure from `bun run check:updater-smoke-evidence`, and a strict synthetic Windows/macOS/Linux pass through `bun run check:updater-smoke-release-evidence -- --dir <temp-updater-smoke-evidence>`.
- Remaining scope: choose and configure production signing providers, execute a real signed release run, complete provider-specific Windows signing setup, complete macOS notarization/stapling evidence, run clean-machine install/uninstall confirmation on target systems, and capture real N-1 to N updater application smoke evidence on all supported updater platforms.

**Pros:**

- Major user trust improvement.

**Cons:**

- Requires secret hygiene, paid certificates, and reliable CI secrets.

## Track 5: Startup Recovery And Coordinator Supervision

**Purpose:** Replace unexplained crashes/dead workers with recoverable app states.

**Files:**

- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/managers/transcription.rs`
- Modify: `src-tauri/src/transcription_coordinator.rs`
- Modify: `src/App.tsx`
- Modify: `src/bindings.ts`
- Modify: `src/i18n/locales/*/translation.json`

### Task 5.1: Structured Bootstrap Errors

- [x] Replace manager `expect` calls with structured `BootstrapError` values.
- [x] Categorize recording, model, transcription, history database, local-LLM, signal-handler, and tray initialization failures.
- [x] Show a recovery screen with safe error code, log directory, Retry, Open App Data, and Reset Settings actions.
- [x] Never include transcript text, prompts, API keys, or clipboard content in recovery text.

**Progress 2026-06-22:**

- Added `StartupStatus` managed state and a generated `get_startup_status` command.
- Core manager, Unix signal-handler, tray icon path/image, and tray construction failures now return labeled startup errors instead of panicking through `expect`/`unwrap`.
- On startup failure, the main window is shown and the frontend checks startup status before onboarding probes call manager-dependent commands.
- Added a localized minimal recovery screen with the failed startup step, sanitized error message, and an Open Logs action.
- Regenerated `src/bindings.ts` from the debug no-default Tauri binary.
- Added Restart Verbatim, Open App Data, and Reset Settings recovery actions. Reset Settings backs up the current stored settings value before writing defaults, then relaunches.
- Regenerated `src/bindings.ts` after adding `reset_settings_to_defaults`.
- Added unit coverage for startup-state failure snapshots and failed native-smoke status serialization.
- Added a packaged-smoke forced failure drill via `VERBATIM_SMOKE_FORCE_STARTUP_FAILURE=1`; the runner validates a failed startup status, safe step/message, main-window creation, and clean smoke-timer exit.
- Remaining scope: run the packaged forced-failure drill on target OSes and add broader real startup-failure drills for manager/tray/resource failures.
- Compile-only verification for forced-failure drill: `cargo test --manifest-path src-tauri/Cargo.toml "startup_state|native_smoke_failed_status" --lib --no-default-features --no-run --quiet`, `cargo check --manifest-path src-tauri/Cargo.toml --no-default-features --quiet`, `bunx tsc --noEmit --pretty false --skipLibCheck --module NodeNext --moduleResolution NodeNext --target ES2022 --types node scripts/native-smoke/run-packaged-smoke.ts`, and `bun run smoke:native -- --help`. Runtime verification: pending native-backend CI job execution on GitHub Actions.

**Verification:**

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml --no-default-features --quiet
bun run lint
bun run check:translations
bun run build
cargo build --manifest-path src-tauri/Cargo.toml --no-default-features --quiet
bun run tauri build -- --debug -- --no-default-features
```

### Task 5.2: Coordinator Supervisor

- [x] Run coordinator under a supervisor that catches panic boundary exits.
- [x] Restart once after a panic and emit a health event.
- [x] After repeated failures, stop accepting dictation commands and show recovery UI.
- [x] Add tests for injected panic, single restart, repeated failure, and successful next operation.

**Progress 2026-06-22:**

- Refactored `TranscriptionCoordinator` into a public supervisor channel plus a single active worker thread that owns the dictation lifecycle state machine.
- Worker panics are caught at the worker boundary and reported back to the supervisor with a generation ID, so stale exits from an already-restarted worker are ignored.
- The supervisor restarts the worker once, emits `transcription-coordinator-health` with `restarted`, and forwards the next command to the new worker when possible.
- A second active-generation failure disables coordinator command handling, emits `transcription-coordinator-health` with `disabled`, and the frontend switches to the recovery screen.
- Added supervisor-state tests for injected panic restart, repeated-failure disablement, stale-exit ignore, and accepting commands after one restart.
- Added an in-memory coordinator health snapshot and a smoke-only panic injection command. Native smoke can now set `VERBATIM_SMOKE_COORDINATOR_PANIC_DRILL=1` and verify that the packaged app reports `restarted` once, then `disabled` after a second active worker panic.
- Remaining scope: run the packaged coordinator panic drill on target OSes.
- Compile-only verification for coordinator panic drill: `cargo test --manifest-path src-tauri/Cargo.toml "worker_panic|supervisor|native_smoke_failed_status" --lib --no-default-features --no-run --quiet`, `cargo check --manifest-path src-tauri/Cargo.toml --no-default-features --quiet`, `bunx tsc --noEmit --pretty false --skipLibCheck --module NodeNext --moduleResolution NodeNext --target ES2022 --types node scripts/native-smoke/run-packaged-smoke.ts`, and `bun run smoke:native -- --help`. Runtime verification: pending native-backend CI job execution on GitHub Actions.

**Verification:**

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml "worker_panic|supervisor" --lib --no-default-features --no-run --quiet
cargo check --manifest-path src-tauri/Cargo.toml --no-default-features --quiet
bun run lint
bun run check:translations
bun run build
```

Note: direct execution of the focused no-default Rust tests still fails on this
Windows setup with `STATUS_ENTRYPOINT_NOT_FOUND`; the focused test binary
compiled successfully with `--no-run`.

## Track 6: Tauri Security Hardening

**Purpose:** Reduce webview and native-capability blast radius.

**Files:**

- Modify: `src-tauri/tauri.conf.json`
- Modify: `src-tauri/capabilities/default.json`
- Modify: `src-tauri/capabilities/desktop.json`
- Create: `src-tauri/capabilities/overlay.json`
- Create: `scripts/validate-tauri-security.ts`

### Task 6.1: CSP

- [x] Inventory required script/style/image/media/connect sources in production.
- [x] Enable a restrictive production CSP.
- [x] Avoid `unsafe-eval`; justify any temporary `unsafe-inline` if Tauri hash/nonces cannot cover it.
- [x] Ensure dynamic locale chunks and asset URLs still load.

### Task 6.2: Asset Protocol And Filesystem Scope

- [x] Replace asset protocol wildcard scope with exact required app-data subpaths for recordings/models/assets.
- [x] Split main window and overlay capabilities.
- [x] Remove duplicated permissions and broad app-data access where commands can mediate access instead.
- [x] Add a validation script that fails CI on `csp: null`, asset `**`, duplicate permissions, or overlay over-permissioning.

**Progress 2026-06-22:**

- Replaced the asset protocol `**` wildcard with exact recording playback scopes for normal app data and portable `Data/recordings`.
- Narrowed frontend fs plugin read scope from all app data to the same installed/portable recordings directories used by history playback.
- Verified JSON parsing for `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json`, and `src-tauri/capabilities/desktop.json`.
- Verified no remaining asset `**` or broad `$APPDATA/**/*` scope in Tauri config/capabilities.
- Added production CSP with no `unsafe-eval`; the remaining `unsafe-inline` is limited to styles because the current Tailwind/Vite styling path requires inline style compatibility.
- Split `recording_overlay` into a core-only capability while keeping plugin permissions on the main window.
- Added `bun run check:tauri-security` and wired it into code quality CI.
- `bun run check:tauri-security` now fails if CSP stops allowing Vite dynamic locale chunks from `'self'` or Tauri asset-protocol image/media URLs via `asset:` and `http://asset.localhost`.
- Playwright Arabic/Hebrew RTL regression coverage exercises lazy locale chunk loading through the real i18n import path.
- Native packaged smoke now fails if the production frontend build is missing
  its main/overlay asset graph or lazy locale chunks.
- Native packaged smoke now asks the packaged app to resolve, size-check, and
  decode/read critical bundled resources including model catalogs, VAD model,
  audio feedback files, and tray/status images.
- Remaining scope: run the release-candidate native smoke on target OSes and
  keep any future full webview CSP violation harness separate from the resource
  availability proof.

**Cons:**

- High regression risk around history audio playback and local assets; must land behind tests.

## Track 7: Secrets And Credential Storage

**Purpose:** Move provider API keys out of the settings store.

**Files:**

- Modify: `src-tauri/src/settings.rs`
- Create: `src-tauri/src/credentials.rs`
- Modify: `src-tauri/src/commands/settings.rs` or provider settings commands
- Modify: `src/components/settings/PostProcessingSettingsApi/ApiKeyField.tsx`
- Modify: `src/i18n/locales/en/translation.json`

### Task 7.1: Credential Abstraction

- [x] Define credential access seam with `get`, `set`, `delete`, and frontend redaction behavior.
- [x] Implement Windows Credential Manager, macOS Keychain, and Linux Secret Service where available through the `keyring` crate.
- [x] Surface credential-store health in settings/diagnostics near provider API-key entry.
- [x] Add explicit Linux fallback behavior: reject persistent remote keys if no secure store is available, unless the user opts into session-only key entry.
- [x] Keep redaction tests for logs and settings debug output.

### Task 7.2: Migration

- [x] On startup, detect non-empty legacy `post_process_api_keys`.
- [x] Migrate keys to credential store.
- [x] Clear legacy values only after successful credential write.
- [x] If migration fails, keep old values, show a diagnostic warning, and do not silently drop keys.

**Progress 2026-06-22:**

- Added `src-tauri/src/credentials.rs` backed by `keyring` so provider API keys are written to OS credential storage instead of the Tauri settings store.
- `get_app_settings` now redacts provider keys before returning settings to the frontend; the UI sees only a stored-secret placeholder.
- `write_settings` and startup settings loading migrate non-empty legacy `post_process_api_keys` values to the OS store and clear the settings value only after the credential write succeeds.
- If the credential write fails, the legacy value is retained and a warning is logged to avoid silent key loss.
- Added a credential-store health command and API-key settings warning/success state so users can see whether the OS credential store is usable on the current platform.
- Credential-store health now includes a sanitized retained legacy key count; API-key settings and Debug diagnostics show a warning when the OS credential store is unavailable and legacy values remain in settings.
- Added opt-in session-only API key entry backed by in-memory state. Runtime dictation and transform provider selection can use session keys, while persistent key entry still requires the OS credential store and generic settings writes clear rejected raw key values instead of persisting them.
- Native packaged smoke now records credential-store health from the isolated profile and fails if any legacy API-key value remains in settings or if the credential probe value leaks into the status message.
- Native packaged smoke now runs a synthetic legacy API-key migration drill against the real OS credential backend when that backend is available. The drill uses a smoke-only provider id, verifies the migrated credential round trip, removes the credential, and fails if the plaintext value remains in settings or smoke status.
- Remaining scope: run packaged/manual key migration checks on target OSes where the credential backend is unavailable in CI or cannot persist the synthetic smoke key.

## Track 8: Model Integrity, Provider Fallback, And Performance Guidance

**Purpose:** Make model downloads verifiable and runtime failures recoverable.

**Files:**

- Modify: `src-tauri/resources/model_catalog.json`
- Modify: `src-tauri/resources/local_llm_catalog.json`
- Modify: `src-tauri/src/managers/model_catalog.rs`
- Modify: `src-tauri/src/local_llm/catalog.rs`
- Modify: `src-tauri/src/managers/model.rs`
- Modify: `src/components/settings/models/ModelsSettings.tsx`
- Create: `docs/MODEL_REQUIREMENTS.md`

- [x] Require SHA-256 for every official downloadable model and local-LLM artifact.
- [x] Allow custom user models, but display them as unverified user-provided assets.
- [x] Add registry tests for HTTPS URLs, checksum presence, license metadata, size, language support, and accelerator compatibility.
- [x] Add provider/model-load diagnostic error codes for missing downloads, provider load failures, accelerator load failures, and provider panics.
- [x] Add controlled CPU fallback when feasible and proven safe for the native engine lifecycle.
- [ ] Publish hardware recommendations only after benchmark runs across representative Windows/macOS/Linux systems.

**Progress 2026-06-22:**

- Confirmed official transcription and local-LLM catalog entries already include SHA-256 values for downloadable artifacts.
- Added a primary transcription catalog integrity test requiring HTTPS-resolved URLs, 64-character hex SHA-256 values, and positive size metadata for every built-in downloadable model.
- Local-LLM catalog already had equivalent HTTPS/SHA-256 test coverage.
- Added transcription catalog tests for required language metadata, normalized score ranges, duplicate language codes, exactly one recommended built-in model, and translation support limited to translation-capable engines.
- Added local-LLM catalog tests requiring runtime, license, quantization, context-window, and language-note metadata.
- Custom user models now display an explicit unverified label and copy explaining that Verbatim has not verified the file, license, checksum, or behavior.
- Added transcription catalog license labels, accelerator-family metadata, a schema test for both fields, and `docs/MODEL_REQUIREMENTS.md` for future model additions.
- Model lifecycle events now include structured diagnostic codes for not-downloaded, provider-load, accelerator-load, and provider-panic failures; provider panics also report the unload/reload fallback action.
- Added a controlled CPU fallback path around native provider load: each load starts by reapplying persisted accelerator settings, retries on CPU only for accelerator-class load failures, does not persist CPU fallback into settings, emits `cpu_after_accelerator_load_failed` on success, and emits `cpu_fallback_failed` if the retry also fails.
- Added pure fallback-decision tests for Whisper GPU fallback, explicit Whisper CPU no-retry, ORT/DirectML fallback, and generic provider failure no-retry.
- Native packaged smoke now runs the fallback-decision drill for Whisper GPU accelerator failure, explicit Whisper CPU no-retry, ORT/DirectML accelerator failure, and generic provider failure. The runner fails if diagnostic codes, CPU retry decisions, or success fallback labels drift.
- Removed the unsupported README Parakeet throughput claim and added `docs/BENCHMARKS.md` plus `bun run check:model-benchmark-evidence`. Code-quality CI now rejects public numeric throughput claims such as `5x real-time` unless structured benchmark evidence is present.
- Added `bun run check:model-benchmark-release-evidence`, which requires reviewed benchmark JSON from representative Windows, macOS, and Linux systems before public hardware recommendations can be published.
- Live verification on 2026-06-23 fails the release evidence gate because no representative platform benchmark result files are present yet, so the hardware-recommendation checkbox remains open.
- Verification completed for the benchmark release evidence gate: `bunx tsc --noEmit --pretty false --skipLibCheck --module NodeNext --moduleResolution NodeNext --target ES2022 --types node scripts/check-model-benchmark-evidence.ts`, `bunx prettier --check scripts/check-model-benchmark-evidence.ts package.json docs/BENCHMARKS.md docs/plans/2026-06-21-audit-remediation-roadmap.md`, `bun run check:model-benchmark-evidence`, and `bun run check:model-benchmark-release-evidence` failing with missing `windows`, `macos`, and `linux` benchmark evidence.
- Remaining scope: default-feature/native CI verification of the actual CPU fallback load path, packaged provider load-failure execution, and benchmark-backed hardware recommendations.
- Compile-only verification: `cargo fmt --manifest-path src-tauri/Cargo.toml`, `cargo test --manifest-path src-tauri/Cargo.toml downloadable_catalog_models_have_https_urls_and_sha256 --lib --no-default-features --no-run --quiet`, `cargo test --manifest-path src-tauri/Cargo.toml "catalog_models_have_complete_language|catalog_recommendation|local_llm_catalog_models_have_runtime" --lib --no-default-features --no-run --quiet`, `cargo test --manifest-path src-tauri/Cargo.toml cpu_fallback --lib --no-default-features --no-run --quiet`, `cargo check --manifest-path src-tauri/Cargo.toml --no-default-features --quiet`, `bun run check:model-benchmark-evidence`, and `git diff --check`. Runtime verification: pending native-backend CI job execution on GitHub Actions.
- Verification blocked: `cargo test --manifest-path src-tauri/Cargo.toml cpu_fallback --lib --no-run --quiet` remains unproven in the native `whisper-rs-sys` Windows build lane on this host. The same short-target/Ninja/native-serial environment used for `model_load_wait` avoided the earlier immediate MSBuild failure but still did not complete within 15 minutes.

## Track 9: Onboarding, Diagnostics, And Platform Readiness

**Purpose:** Make first run prove the app can actually transcribe and insert.

**Files:**

- Modify: `src/components/onboarding/*`
- Modify: `src/components/settings/debug/DebugSettings.tsx`
- Create: `src/components/settings/debug/DiagnosticsPanel.tsx`
- Modify: `src-tauri/src/commands/audio.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/shortcut/*`
- Modify: `src/i18n/locales/en/translation.json`

- [x] Add first-run shortcut capture and conflict detection.
- [x] Add live microphone level and selected-device test.
- [x] Add test dictation with replay/discard option.
- [x] Insert into a controlled field or clearly fall back to copy-only.
- [x] Add Linux environment readiness: X11/Wayland, desktop environment, available insertion helpers, clipboard helper, AT-SPI, and tray status.
- [x] Add diagnostics dashboard: permissions, registered shortcuts, selected model, accelerator, insertion method, update status, storage paths, logs.

**Progress 2026-06-22:**

- Added a Linux readiness command that reports session type, desktop environment, helper availability for `wl-copy`, `wtype`, `kwtype`, `dotool`, `ydotool`, and `xdotool`, the helper Verbatim would prefer for clipboard paste shortcuts and direct input, AT-SPI environment availability, and tray status.
- Surfaced the readiness result beside the Linux paste-method setting so missing helper coverage is visible before users rely on insertion.
- Added a Debug settings diagnostics panel covering startup status, platform permissions, registered shortcuts, selected model integrity, accelerator availability, insertion settings, update-check configuration, history/recording/private-session storage state, credential-store status, Linux helper warnings, and app/log paths.
- Native packaged smoke now records the Linux readiness snapshot and the Ubuntu Xvfb lane installs/asserts `xdotool` as the X11 direct-input and key-combo helper.
- Remaining scope: packaged Wayland helper smoke tests and controlled Linux paste targets.

**Progress 2026-06-22 (Shortcut Readiness):**

- Added a first-run shortcut readiness screen after model selection for new users.
- Reused the production shortcut capture control for the recording shortcut, so backend registration errors still surface through the same `changeBinding` path used by Settings.
- Added an onboarding readiness gate for missing or duplicate recording shortcuts before entering the main app.
- Added the new onboarding shortcut strings across all locale files.
- Remaining scope: packaged first-success drills after the microphone and test dictation readiness screens.
- Verification completed: `bun run build`, `bun run check:translations`, and `bun run lint`.

**Progress 2026-06-22 (Microphone Readiness):**

- Added first-run microphone readiness after shortcut setup, with selected-device display and the existing microphone selector available before entering the app.
- Added backend microphone-test commands that open the selected microphone stream for preview, emit existing `mic-level` updates, and stop the preview without creating a recording, transcript, or history row.
- Added a live level meter and readiness states for stream-open and detected input.
- Stabilized parent onboarding callbacks so permission completion cannot loop when first-run onboarding remains active.
- Added mocked first-run Playwright coverage for model setup, shortcut readiness, microphone preview, live input detection, and entering the main app.
- Remaining scope: packaged first-success drills.
- Verification completed: `cargo fmt --manifest-path src-tauri/Cargo.toml`, `cargo check --manifest-path src-tauri/Cargo.toml --no-default-features --quiet`, `bun run build`, `bun run check:translations`, `bun run lint`, and `$env:PLAYWRIGHT_USE_SYSTEM_CHROME='1'; bunx playwright test tests/app.spec.ts -g 'first-run onboarding verifies shortcut and microphone readiness' --project=chromium --timeout=60000`.

**Progress 2026-06-22 (Test Dictation Readiness):**

- Added an isolated first-run test dictation step after microphone readiness. It records under a dedicated onboarding binding, transcribes the captured samples, and returns the text without writing history, saving WAV artifacts, or inserting into another application.
- Added first-run transcript review with Record again, Copy test text, and Discard and continue controls. The fallback is intentionally copy-only instead of automatic paste during onboarding, because no external foreground target is controlled there.
- Added backend onboarding dictation start, stop, cancel, and copy commands and wired them into the Tauri command registry and TypeScript binding shim.
- Extended first-run Playwright coverage through model setup, shortcut readiness, microphone preview, test dictation transcription, copy-only fallback, and entering the main app.
- Remaining scope: packaged first-success drills that prove the onboarding path against a real native app/model install.
- Verification completed: `cargo check --manifest-path src-tauri/Cargo.toml --no-default-features --quiet`, `bun run check:translations`, `git diff --check`, `bun run build`, `bun run lint`, and `$env:PLAYWRIGHT_USE_SYSTEM_CHROME='1'; bunx playwright test tests/app.spec.ts -g 'first-run onboarding verifies shortcut and microphone readiness' --project=chromium --timeout=60000`.

## Track 10: Accessibility And Internationalization Assurance

**Purpose:** Protect keyboard, screen-reader, contrast, motion, and RTL behavior.

**Files:**

- Modify: `playwright.config.ts`
- Create: `tests/accessibility/`
- Modify: shared UI components under `src/components/ui/`
- Modify: `src/lib/utils/rtl.ts`

- [x] Add axe accessibility checks for onboarding, settings, history, model selector, post-processing, diagnostics, and overlay.
- [x] Add keyboard-only navigation tests for every settings section.
- [x] Add live-region assertions for recording, processing, inserted, copied, cancelled, and paste-failed states.
- [x] Add manual release checklist entries for NVDA, VoiceOver, and Orca.
- [x] Add Arabic/Hebrew mixed-direction regression cases.

**Progress 2026-06-22:**

- Added an Accessibility Smoke section to `docs/RELEASE_CHECKLIST.md` requiring NVDA, VoiceOver, and Orca checks across onboarding, settings navigation, recording, cancellation, paste failure recovery, and history review.
- Added keyboard-only, live-state, and Arabic/Hebrew mixed-direction reminders to the release checklist so manual release review covers the known gaps until automated coverage exists.
- Added `scripts/check-accessibility-smoke-evidence.ts`, exposed as `bun run check:accessibility-smoke-evidence` and `bun run check:accessibility-smoke-release-evidence`, to validate retained Windows/NVDA, macOS/VoiceOver, and Linux/Orca smoke JSON before release approval.
- The transient recording overlay now exposes its state through a polite `role="status"` region with a state-specific accessible label.
- Added a Playwright regression that asserts recording, processing, inserted, copied, cancelled, and paste-failed overlay states have live status names.
- Settings sidebar entries are now semantic buttons with `aria-current` on the active section, and Playwright activates every enabled settings section by keyboard, including the conditional Post Process section.
- Added an axe Playwright suite for onboarding permissions, General/Models/History/Post Process/Debug settings surfaces, and the recording overlay; shared controls now expose accessible names and light-mode muted/status colors meet contrast in those covered states.
- Added Arabic and Hebrew RTL regression coverage that waits for document `dir`/`lang` synchronization, verifies translated sidebar controls remain reachable, and checks the first viewport avoids horizontal overflow.
- Remaining scope: full assistive-technology manual certification during release smoke.
- Verification completed for the accessibility-smoke evidence verifier: `bunx tsc --noEmit --pretty false --skipLibCheck --module NodeNext --moduleResolution NodeNext --target ES2022 --types node scripts/check-accessibility-smoke-evidence.ts`, `bunx prettier --check scripts/check-accessibility-smoke-evidence.ts package.json docs/RELEASE_CHECKLIST.md docs/QA.md docs/plans/2026-06-21-audit-remediation-roadmap.md`, `bun run check:accessibility-smoke-evidence -- --help`, an expected missing-evidence failure from `bun run check:accessibility-smoke-evidence`, and a strict synthetic Windows/macOS/Linux pass through `bun run check:accessibility-smoke-release-evidence -- --dir <temp-accessibility-smoke-evidence>`.

## Track 11: Supply Chain, SBOM, And Dependency Policy

**Purpose:** Make dependency state reproducible and auditable.

**Files:**

- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`
- Create: `deny.toml`
- Modify: `.github/workflows/code-quality.yml`
- Modify: `.github/workflows/release.yml`
- Create: `docs/DEPENDENCY_POLICY.md`

- [x] Pin Git dependencies to immutable `rev` values.
- [x] Document why any Git dependency remains necessary.
- [x] Add `cargo-deny` for advisories, licenses, bans, and allowed sources.
- [x] Add frontend audit policy that respects Bun lockfile integrity.
- [x] Generate SBOM and provenance metadata during release.
- [x] Fail release builds on unauthorized sources or missing SBOM/provenance metadata.

**Progress 2026-06-22:**

- Replaced branch-based Rust Git dependencies with immutable commit `rev` pins.
- Added `bun run check:cargo-git-pins` and wired it into code-quality CI.
- Added `deny.toml` and `bun run check:rust-dependency-policy` for cargo-deny advisory, license, ban, and source checks.
- Updated yanked/vulnerable lockfile entries for the wasm-bindgen/js-sys family, `rustls-webpki`, and `tar`.
- Added `docs/DEPENDENCY_POLICY.md` documenting dependency rules, required Git dependencies, Bun lockfile policy, and SBOM/provenance expectations.
- Added `scripts/generate-release-metadata.ts` plus release workflow steps that upload `SBOM.spdx.json` and `RELEASE_PROVENANCE.json`, then fail manifest publication if either metadata asset is missing.
- Release package builds now generate GitHub Artifact Attestations with `actions/attest@v4` for the local packaged `.exe`, `.msi`, `.dmg`, and `.deb` outputs before publication. The reusable build job prepares a subject checksum file from bundle outputs and grants only the required `id-token: write` and `attestations: write` permissions for attestation signing.
- `docs/DEPENDENCY_POLICY.md`, `docs/SIGNING.md`, and generated release notes now document Artifact Attestations as the signed provenance signal for packaged desktop artifacts, while keeping `RELEASE_PROVENANCE.json` as a readable release-context summary.
- Attested release approval now requires retained `gh attestation verify` status JSON for each packaged desktop artifact through `bun run check:attested-release-evidence` or the combined release readiness `--require-attestations` flag.
- Remaining scope: platform code signing, notarization, timestamp evidence, and running the first real published-asset attestation verification remain tracked in the signing/release smoke tracks.
- Verification completed: `bunx prettier --check .github/workflows/build.yml .github/workflows/release.yml docs/DEPENDENCY_POLICY.md docs/SIGNING.md docs/plans/2026-06-21-audit-remediation-roadmap.md`, `bun run check:release-metadata`, and `bun run check:public-hygiene -- --all`.

## Track 12: Architecture, Threat Model, And Contributor Docs

**Purpose:** Make the project easier to modify safely.

**Files:**

- Create: `docs/ARCHITECTURE.md`
- Create: `docs/THREAT_MODEL.md`
- Create: `docs/DATA_FLOW.md`
- Modify: `docs/BUILD.md`
- Modify: `CONTRIBUTING.md`
- Modify: `docs/adr/0001-engine-provider-architecture.md`

- [x] Document command-event flow, manager responsibilities, coordinator state machine, insertion transaction, clipboard behavior, history retention, update flow, and provider architecture.
- [x] Document trust boundaries: webview, Tauri commands, filesystem, updater, providers, clipboard, accessibility APIs, model downloads.
- [x] Document how generated bindings are updated after adding backend commands.
- [x] Document real versus mocked transcription tests.
- [x] Add first-contribution path with exact commands and expected runtime prerequisites.

**Progress 2026-06-22:**

- Added `docs/ARCHITECTURE.md` covering backend/frontend domains, command-event flow, dictation flow, settings/secrets, provider direction, and release/update flow.
- Added `docs/DATA_FLOW.md` covering audio, transcript text, remote providers, clipboard, history, external scripts, updater, and logs.
- Added `docs/THREAT_MODEL.md` covering assets, trust boundaries, risks, controls, non-goals, and review checklist.
- Updated `CONTRIBUTING.md` to link the architecture/data-flow/threat-model docs and call out generated binding updates.
- Updated `docs/BUILD.md` with the debug build path for regenerating `src/bindings.ts`.
- Track 12 documentation scope is complete; future updates should keep these docs current as the provider architecture and native test harness evolve.

## Track 13: Settings Refactor

**Purpose:** Reduce long-term settings fragility after safety work lands.

**Safety-branch status:** Split out of `codex/phase11-safety-only` after review. The broad settings-domain migration, domain document commands, domain write validation, `settings_domains.rs`, `settingsDocument.ts`, generated binding-export helper, and legacy flat settings command removal belong in the follow-up `codex/phase13-settings-refactor` branch.

- [ ] Split settings into versioned domains: general, audio, insertion, privacy, models, post-processing, diagnostics, adaptive, shortcuts.
- [ ] Add migrations with tests for existing settings files.
- [ ] Keep generated binding regeneration immediately after adding backend commands or types.
- [x] Do not mix this refactor into cancellation/insertion safety PRs.

**Progress 2026-06-23 (Split):**

- Safety branch restored flat `get_app_settings` and `get_default_settings` transport, flat settings persistence, and flat `AppSettings` frontend bindings while keeping safety-specific settings fields such as history/recording retention controls.
- Track 13 implementation work is intentionally deferred to `codex/phase13-settings-refactor` so safety fixes can be reviewed and rolled back independently.
- Runtime verification for any future Track 13 tests must come from native-backend CI or target OS runners; local Windows `--no-run` checks are compile-only evidence.

## Track 14: Long-Term Product Trust

**Purpose:** Raise ordinary-user readiness after core safety and release gates are stable.

- [ ] Optional encrypted local history after keychain and zero-retention controls are stable.
- [ ] Reproducible builds after signing/provenance/SBOM are stable.
- [ ] Broader architectures only when CI and manual QA capacity exist.
- [x] Per-application privacy exclusions after the insertion transaction and target fingerprinting are stable.
- [x] Local performance profiling and model recommendation without telemetry.
- [x] Safe local extension points that pass text over stdin or structured local IPC, never command-line arguments.

**Progress 2026-06-22:**

- `PasteMethod::ExternalScript` is the supported local extension point. It spawns the configured script without transcript command-line arguments, writes dictated text to stdin, nulls stdout/stderr, suppresses script output in failure text, polls operation cancellation, and kills the child process if cancellation arrives.
- `docs/DATA_FLOW.md` documents that external scripts receive dictated text through stdin and must not receive dictated text as a command-line argument.
- Added `scripts/model-performance-profile.ts` plus `benchmark:model:record` and `benchmark:model:recommend` for local-only benchmark capture and model ranking from local JSON files. The helper records manually measured deterministic fixture timings, rejects malformed duration input, performs no telemetry or network calls, and warns that local ranking output is not public hardware guidance.
- `docs/BENCHMARKS.md` now documents the recorder/recommender flow and keeps public hardware recommendations gated on reviewed benchmark evidence across representative Windows, macOS, and Linux systems.
- Per-application private target patterns now trigger a pre-recording privacy check even when broader context awareness is disabled. If the foreground target matches a private-app exclusion, Verbatim clears the shortcut context, blocks before microphone capture, emits a `target_privacy_excluded` recording error, and surfaces a localized warning in the app. `docs/PRIVACY.md` documents the distinction from private session.
- Remaining scope: optional encrypted history, reproducible builds, and broader architectures remain deferred until the earlier release/signing/privacy gates are stable.
- Compile-only verification: `cargo test --manifest-path src-tauri/Cargo.toml "external_script_invocation|external_script_failure_message|cancellation_guard" --lib --no-default-features --no-run --quiet`, `cargo check --manifest-path src-tauri/Cargo.toml --no-default-features --quiet`, `bunx prettier --check docs/plans/2026-06-21-audit-remediation-roadmap.md docs/DATA_FLOW.md`, `bunx prettier --check scripts/model-performance-profile.ts docs/BENCHMARKS.md docs/plans/2026-06-21-audit-remediation-roadmap.md package.json`, `bunx tsc --noEmit --pretty false --skipLibCheck --module NodeNext --moduleResolution NodeNext --target ES2022 --types node scripts/model-performance-profile.ts scripts/check-model-benchmark-evidence.ts`, `bun run benchmark:model:recommend`, `bun run benchmark:model:record -- --model-id smoke-model --engine test --accelerator cpu --fixture-id deterministic-speech-fixture-v1 --audio-seconds 30 --sample-rate-hz 16000 --duration-ms=6000,6100,5950 --out $env:TEMP\verbatim-model-profile-test.json`, `bun run check:model-benchmark-evidence`, `cargo fmt --manifest-path src-tauri/Cargo.toml`, `cargo test --manifest-path src-tauri/Cargo.toml "target_privacy_exclusion|context_runtime" --lib --no-default-features --no-run --quiet`, `bun run check:translations`, `bun run lint`, `bun run build`, and `bunx prettier --check "src/i18n/locales/**/*.json" src/App.tsx docs/PRIVACY.md docs/plans/2026-06-21-audit-remediation-roadmap.md`. Runtime verification: pending native-backend CI job execution on GitHub Actions.

## Recommended Milestones

### Milestone A: Truthful Release Baseline

Exit criteria:

- Privacy and security docs published.
- Diagnostics visible.
- Bug template collects platform diagnostics.
- Release finalizer requires expected desktop artifacts.
- SHA-256 manifest published.
- Release page separates desktop and Android.

### Milestone B: Sensitive Text Safety

Exit criteria:

- Cancellation blocks all side effects.
- Standard and adaptive insertion use one destination-safe transaction.
- Clipboard restoration is ownership-aware.
- External scripts receive stdin.
- Regression tests cover cancellation, focus change, and clipboard mutation.

### Milestone C: Native Release Gate

Exit criteria:

- Windows/macOS/Linux production modules compile in required CI.
- Release candidate packaged smoke runs on all release platforms.
- Previous-version update test is required.
- Failure artifacts include logs and screenshots.

### Milestone D: Privacy And Startup Resilience

Exit criteria:

- Zero-retention and private-session controls exist and are tested.
- API keys migrate to credential stores.
- Manager startup failures show recovery UI.
- Coordinator panic is supervised and surfaced.

### Milestone E: Sign And Harden

Exit criteria:

- Windows EXE/MSI are Authenticode-signed and timestamped.
- macOS app/DMG are Developer ID signed, notarized, and stapled.
- Tauri CSP enabled.
- Asset protocol and capabilities are least-privilege.
- SBOM/provenance/advisory gates run in release.

## Verification Policy

Small docs/UI changes:

```bash
bunx prettier --check README.md CONTRIBUTING.md docs .github src
bun run check:translations
bun run lint
bun run build
git diff --check
```

Rust backend changes:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml <focused_module_or_test_name> --lib
```

Windows backend changes:

```powershell
bun run check:backend:windows
bun run test:backend:windows
```

Release workflow changes:

```bash
bun run check:version
bun run check:readme-release
bun scripts/check-public-hygiene.ts --all
```

Manual gates required before claiming ordinary-user readiness:

- Clean install and uninstall on Windows, macOS, Ubuntu.
- N-1 to N update on each platform.
- Permission deny/grant/revoke/regrant.
- Shortcut registration and conflict behavior.
- Audio input, silence, noise, and unplug/replug.
- Cancel during every pipeline stage.
- Focus switch during inference.
- Clipboard mutation during paste.
- Startup with corrupt settings and corrupt history DB.
- Logs reviewed for transcripts, prompts, clipboard content, and secrets.

## Stop Rules

- Do not sign or promote a release as ordinary-user ready while cancellation/insertion safety is known incomplete.
- Do not add new Linux package formats until X11/Wayland helper behavior is tested and documented.
- Do not enable remote provider features by default.
- Do not present key redaction as encryption.
- Do not merge broad settings refactors into safety-critical PRs.
- Do not claim native smoke coverage unless the packaged app was actually launched on the target OS.
