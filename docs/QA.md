# QA

## Native Packaged Smoke

The `native smoke` workflow builds unsigned packaged desktop artifacts on Windows x64, macOS ARM64, and Ubuntu x64, then launches the packaged executable in an isolated profile.

The release workflow calls the same native smoke workflow before finalizing a desktop release, so draft publication is blocked if packaged startup/resource smoke fails on any release platform.

The workflow also runs `bun run smoke:installer` against the produced installer
or package. That installs the Windows NSIS artifact into a temporary normal
current-user install with shortcuts disabled, copies the macOS app out of the
DMG, or installs the Ubuntu `.deb`, then delegates to the same packaged smoke
runner against the installed app path. Cleanup also exercises the generated
Windows uninstaller and Ubuntu package removal path.

The smoke runner verifies:

- The packaged app starts with production resources.
- Startup reaches the Tauri setup path and exits cleanly through `VERBATIM_SMOKE_EXIT_AFTER_MS`.
- The app writes `*.status.json` through `VERBATIM_SMOKE_STATUS_PATH` after settings load, main-window creation, tray initialization, updater plugin registration, single-instance plugin registration, close-to-tray handler registration, and debug-mode override.
- The app creates a smoke-only local `verbatim-smoke-model.bin` through `VERBATIM_SMOKE_MODEL_FIXTURE=1`, auto-selects it as a custom downloaded model, and confirms the selected model has no remote URL.
- When `--smoke-microphone <name>` or `VERBATIM_SMOKE_SELECTED_MICROPHONE` is set, the app persists that smoke-only microphone selection into the isolated profile before the recording manager initializes, and the runner fails if status does not report the same selected microphone.
- The app verifies clean-profile retention defaults in the isolated profile: history enabled, recordings enabled, `history_limit = 5`, `recording_retention_period = preserve_limit`, zero history rows, and zero recording files.
- The app runs the production dictation storage-policy seam for default, recordings-disabled, history-disabled, and private-session cases, and the runner fails if any case would persist history/WAV contrary to policy.
- The app reports credential-store health for the isolated profile and the runner fails if any legacy API-key value remains in settings or if the credential probe value appears in status output.
- The app runs a model-load fallback decision drill covering accelerator-class failures and generic provider failures; the runner fails if CPU retry decisions or fallback labels drift.
- The runner checks the production frontend build contains the main and overlay asset graph plus lazy locale chunks.
- The packaged app resolves, size-checks, and decodes/reads critical bundled resources: model catalogs, VAD model, audio feedback files, and tray/status images.
- On Linux, the packaged app reports session/helper readiness and the Ubuntu Xvfb smoke lane asserts a usable `xdotool` helper for X11 direct input and key combos.
- The app writes and verifies a deterministic 16 kHz WAV fixture through `VERBATIM_SMOKE_AUDIO_FIXTURE_PATH`.
- When enabled, the controlled desktop-target drill launches a disposable text target and writes `controlled-desktop-targets.json`; text entry and clipboard mutation remain opt-in and use only synthetic markers in isolated sessions.
- When `--app-insertion-drills <path>` is supplied, the runner validates full app-driven insertion race evidence. `--require-app-insertion-drills` fails the smoke if that evidence is missing, which prevents release smoke from treating the controlled-target preflight as proof that Verbatim itself pasted into the target.
- A forced startup failure through `VERBATIM_SMOKE_FORCE_STARTUP_FAILURE=1` writes a failed startup status, shows the recovery window path, and exits cleanly through the smoke timer.
- A forced coordinator panic drill through `VERBATIM_SMOKE_COORDINATOR_PANIC_DRILL=1` verifies worker supervision reports `restarted` once, then `disabled` after a second active worker panic.
- A duplicate process exits cleanly through the single-instance path.
- Logs, stdout, stderr, a JSON summary, and best-effort screenshots are collected as workflow artifacts.

The smoke runner intentionally uses `--no-tray` for the deterministic CI launch. Tray construction is still checked through app status, model selection is checked without external providers or downloads, bundled resource availability is checked by the packaged app, Linux X11 helper readiness is checked under Ubuntu Xvfb, credential health is checked without provider secrets, credential migration is checked against the real OS credential backend when that backend is available, retention policy is checked without a live microphone/model, model-load fallback policy is checked without forcing a native engine failure, and the audio fixture is written by the packaged app. The controlled desktop-target drill is a launch/focus/text-entry/clipboard-marker preflight; full Verbatim-driven paste, focus-switch, and clipboard-race automation against those targets still requires the broader packaged desktop smoke lane tracked in the audit roadmap. Visible tray interaction, close-to-tray behavior, updater UI, N-1 to N updater application, Wayland helper behavior, OS virtual audio input, real model inference, real provider load-failure execution, real dictation zero-retention drills, full webview CSP violation drills, real OS credential migration on any CI platform whose credential backend is unavailable, and real image/file/rich-text clipboard races also remain open.

Run locally after building a package:

```bash
bun run smoke:native -- --app path/to/Verbatim --artifact-dir native-smoke-artifacts
```

On Linux CI, run it inside a display session:

```bash
xvfb-run -a bun run smoke:native -- --artifact-dir native-smoke-artifacts
```

Useful options:

- `--app <path>` points at the packaged executable when auto-discovery is not enough.
- `--artifact-dir <path>` chooses where logs, screenshots, and `native-smoke-summary.json` are written.
- `--timeout-ms <number>` changes the per-launch timeout.
- `--skip-single-instance` runs only the first-launch smoke.
- `--skip-startup-failure` skips the forced startup-failure recovery smoke.
- `--skip-coordinator-panic` skips the forced coordinator panic supervision smoke.
- `--smoke-microphone <name>` selects a named microphone in the isolated smoke profile before the audio manager initializes. Use `default` to require the default-input path.
- `--desktop-target-drill` runs the controlled desktop text-target preflight after packaged startup smoke.
- `--require-desktop-target` fails the smoke if the controlled target cannot be launched or focused.
- `--allow-text-entry` permits the controlled-target drill to type a synthetic marker into the disposable target and verify it was saved.
- `--allow-clipboard-write` permits the controlled-target drill to write and read back a synthetic clipboard marker. Use it only in isolated desktop sessions.
- `--app-insertion-drills <path>` validates full app-driven race evidence JSON.
- `--require-app-insertion-drills` fails if the app-driven race evidence JSON is missing.

App-driven insertion race evidence must use this shape:

```json
{
  "schema_version": 1,
  "cases": [
    {
      "case": "focus_switch_during_inference_blocks_insertion",
      "app_driven": true,
      "passed": true,
      "desktop_target": "notepad",
      "inference_started": true,
      "focus_switched_before_insertion": true,
      "insertion_blocked": true,
      "failures": []
    },
    {
      "case": "clipboard_mutation_during_paste_preserves_user_clipboard",
      "app_driven": true,
      "passed": true,
      "desktop_target": "notepad",
      "paste_attempted": true,
      "clipboard_mutated_after_verbatim_write": true,
      "user_clipboard_preserved": true,
      "user_clipboard_contents_recorded": false,
      "failures": []
    }
  ]
}
```

The evidence must come from a disposable desktop session where the packaged app
performs the insertion attempt. It must not record real clipboard contents.

Validate retained smoke artifacts before treating them as release evidence:

```bash
bun run check:native-smoke-artifacts -- --dir native-smoke-artifacts
```

For release candidates that enabled every opt-in lane, require those artifacts
explicitly:

```bash
bun run check:native-smoke-artifacts -- --dir native-smoke-artifacts --require-installer --require-desktop-target --require-virtual-audio --require-app-insertion-drills
```

The runner isolates profile paths via `APPDATA`/`LOCALAPPDATA` on Windows and `XDG_CONFIG_HOME`/`XDG_DATA_HOME`/`XDG_CACHE_HOME` on Unix-like platforms. It also sets `VERBATIM_NO_GTK_LAYER_SHELL=1` to keep Linux overlay setup deterministic under CI. The smoke model fixture is intentionally a metadata-only placeholder and must not be used for inference.

Virtual-audio preflight:

```bash
bun run smoke:virtual-audio -- --artifact-dir native-smoke-artifacts --device-name "VB-CABLE Output"
bun run smoke:virtual-audio -- --artifact-dir native-smoke-artifacts --create-linux-pulse-source --require
```

The helper writes `virtual-audio-input.json`. On Linux, `--create-linux-pulse-source` uses `pactl load-module` to create a session-scoped source named `verbatim_smoke_source` and reports cleanup commands. On Windows and macOS, provide the already-installed virtual input name with `--device-name`, then pass the reported `smoke_microphone_arg` to `bun run smoke:native -- --smoke-microphone <name>`. Use `--cleanup-from native-smoke-artifacts/virtual-audio-input.json` to unload Linux modules and write `virtual-audio-cleanup.json`. This is a virtual-input provisioning step; real fixture playback and packaged inference orchestration remain part of the broader smoke lane.

Use `--input-from native-smoke-artifacts/virtual-audio-input.json --play-fixture <path>` to play a WAV fixture into the Linux virtual sink and write `virtual-audio-playback.json`. This proves the generated fixture can be routed into the provisioned audio graph, but it does not yet prove the packaged app recorded and transcribed it.

The `native smoke` workflow exposes the same path through `virtual-audio-preflight`. Linux creates `verbatim_smoke_source` before packaged smoke, passes it to `--smoke-microphone`, plays the app-generated `audio_fixture_path` into the virtual sink after packaged smoke, exports the microphone for installer smoke through `VERBATIM_SMOKE_SELECTED_MICROPHONE`, and unloads the PulseAudio modules in an `always()` cleanup step. Windows and macOS require `virtual-audio-device-name` because the virtual audio driver must already be installed in the runner image or disposable desktop session.

Before uploading workflow artifacts, the workflow runs
`bun run check:native-smoke-artifacts -- --dir native-smoke-artifacts --require-installer`.
It also adds `--require-desktop-target` and `--require-virtual-audio` when the
matching opt-in workflow inputs are enabled, so a green native-smoke job proves
that required retained evidence exists.

The release workflow passes `native_smoke_desktop_target_drill`,
`native_smoke_virtual_audio_preflight`, and
`native_smoke_virtual_audio_device_name` through to the reusable native-smoke
workflow. When virtual audio is enabled for release smoke, the device name is
required because release smoke runs Windows and macOS in addition to Linux.

## Native Backend Branch Protection

The `native backend` workflow must be required on `main` once the workflow has
landed on the default branch and the repository owner approves the repo setting
change.

Required status check contexts:

- `Windows x64 production backend`
- `macOS ARM64 production backend`
- `Ubuntu x64 production backend`

Do not mark the audit item complete until GitHub branch protection for `main`
returns these contexts in `required_status_checks.contexts`. Verify with:

```bash
bun run check:branch-protection
```

## Accessibility Release Smoke

Manual assistive-technology certification must retain one
`accessibility-smoke*.json` file per platform. The release evidence gate expects
Windows with NVDA, macOS with VoiceOver, and Linux with Orca.

```json
{
  "schema_version": 1,
  "platform": "windows",
  "assistive_technology": "NVDA",
  "tester": "release-reviewer",
  "tested_at": "2026-06-23T12:00:00Z",
  "version": "0.8.8",
  "onboarding_verified": true,
  "settings_navigation_verified": true,
  "recording_verified": true,
  "cancellation_verified": true,
  "paste_failure_recovery_verified": true,
  "history_review_verified": true,
  "keyboard_only_navigation_verified": true,
  "live_states_announced_without_transcript_leak": true,
  "rtl_mixed_direction_verified": true,
  "failures": []
}
```

For release approval, validate the retained evidence:

```bash
bun run check:accessibility-smoke-release-evidence -- --dir <accessibility-smoke-evidence-dir>
```

## Combined Release Evidence Check

After collecting release assets and retained release-smoke evidence, run the
combined gate so all required evidence directories are validated together:

```bash
bun run check:release-readiness-evidence -- --release-assets-dir <downloaded-release-assets> --native-smoke-dir <native-smoke-win32-artifact-dir> --native-smoke-dir <native-smoke-darwin-artifact-dir> --native-smoke-dir <native-smoke-linux-artifact-dir> --install-smoke-dir <install-smoke-evidence-dir> --updater-smoke-dir <updater-smoke-evidence-dir> --accessibility-smoke-dir <accessibility-smoke-evidence-dir>
```

Add `--signed` for signed releases. Add `--require-attestations` when retained
GitHub Artifact Attestation verification JSON is required for packaged desktop
artifacts. Add `--require-desktop-target`, `--require-virtual-audio`,
`--require-app-insertion-drills`, `--require-benchmarks`, and
`--require-branch-protection` when those gates apply to the candidate.

The combined check requires native-smoke summaries for `win32`, `darwin`, and
`linux`, matching Node's platform names in `native-smoke-summary.json`.

When `--require-branch-protection` is present, the check uses `gh api` under the
hood and fails if `main` is unprotected or if any native backend context is
missing. Applying branch protection is a repository setting change and requires
explicit repository-owner approval.
