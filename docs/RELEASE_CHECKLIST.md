# Release Checklist

Use this checklist before publishing a desktop release.

## Before Triggering Release

- Confirm `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json` use the same version.
- Confirm branch protection requires the native backend status checks on `main`: `Windows x64 production backend`, `macOS ARM64 production backend`, and `Ubuntu x64 production backend`.
- Run `bun run check:version`.
- Run `bun run check:translations`.
- Run `bun run check:tauri-security`.
- Run `bun run format:check`.
- Run `bun run lint`.
- Run `bun run build`.
- Run the smallest relevant backend check for the changed area.
- Confirm no private notes, local paths, credentials, or internal assistant artifacts are tracked.

## Workflow Inputs

- Use `sign_binaries: true` only after the signing secrets and provider accounts are confirmed current.
- For signed releases, set `signing_identity_label` to the public publisher or Developer ID identity expected in the platform signature.
- Use `sign_binaries: false` only for unsigned preview releases, and keep the release notes explicit about unsigned status.
- For signed releases, confirm the build workflow's signing prerequisite step passed before package compilation.
- For signed releases, confirm Windows Authenticode verification and macOS codesign/Gatekeeper/stapling verification passed in CI.
- Set `native_smoke_desktop_target_drill: true` when the release candidate must retain controlled desktop-target evidence.
- Set `native_smoke_virtual_audio_preflight: true` only when a Windows/macOS virtual microphone is available, and set `native_smoke_virtual_audio_device_name` to that input name.

## Required Desktop Assets

- `Verbatim_<version>_x64-setup.exe`
- `Verbatim_<version>_x64_en-US.msi`
- `Verbatim_<version>_aarch64.dmg`
- `Verbatim_<version>_amd64.deb`
- `latest.json`
- Updater archives for Windows x64, macOS Apple Silicon, and Linux x64
- `.sig` files for every updater archive referenced by `latest.json`
- `SHA256SUMS.txt`
- `RELEASE_MANIFEST.json`

## Release Verification

- Confirm the release workflow's `release-smoke` job passed on Windows, macOS, and Linux before publishing the draft.
- Download and retain the `native-smoke-*` workflow artifacts for the release candidate.
- Confirm each native smoke artifact contains `native-smoke-summary.json`, first-launch status logs, installer smoke logs, and screenshots or screenshot-skip notes.
- Run `bun run check:native-smoke-artifacts -- --dir <native-smoke-artifact-dir> --require-installer`, adding `--require-desktop-target`, `--require-virtual-audio`, and `--require-app-insertion-drills` for every opt-in lane enabled on the candidate.
- If `desktop-target-drill` was enabled, confirm `controlled-desktop-targets.json` reports the disposable target launched, focused, and, in isolated desktop sessions, saved the synthetic text-entry marker without reading user clipboard contents.
- If `virtual-audio-preflight` was enabled, confirm `virtual-audio-input.json`, `virtual-audio-playback.json`, and `virtual-audio-cleanup.json` are present for Linux; `virtual-audio-playback.json` must show the deterministic WAV fixture was played into the provisioned virtual sink, and cleanup must unload the reported PulseAudio modules.
- Confirm `latest.json` has `windows-x86_64`, `darwin-aarch64`, and `linux-x86_64` platform entries.
- Confirm each updater entry has a non-empty `signature`.
- Confirm each updater URL points to an uploaded release asset.
- Confirm `SHA256SUMS.txt` contains every release asset except itself and `RELEASE_MANIFEST.json`.
- Confirm `RELEASE_MANIFEST.json` includes file size, SHA-256, updater platform key, updater signature presence, signing status, public signing identity when signed, provenance field, and SBOM field for every asset.
- Run `bun run check:release-evidence -- --dir <downloaded-release-assets>`.
- For attested releases, retain `attestations/<asset>.attestation.json` for every packaged desktop artifact and run `bun run check:attested-release-evidence -- --dir <downloaded-release-assets>`.
- For signed releases, run `bun run check:signed-release-evidence -- --dir <downloaded-release-assets>`.
- Install each platform artifact on a clean machine or VM, retain `install-smoke*.json`, and run `bun run check:install-smoke-release-evidence -- --dir <install-smoke-evidence-dir>`. For signed releases, run `bun run check:install-smoke-signed-release-evidence -- --dir <install-smoke-evidence-dir>`.
- On Windows, confirm installer smoke covered both default app-data preservation and explicit `/DELETEAPPDATA` removal, then verify the same install/uninstall behavior on a clean VM.
- Run one local transcription.
- Run one insertion into a plain text target.
- Install the previous release on each supported updater platform, verify it detects and applies the update, retain `updater-smoke*.json`, and run `bun run check:updater-smoke-release-evidence -- --dir <updater-smoke-evidence-dir>`.
- Before approving the release, run `bun run check:release-readiness-evidence -- --release-assets-dir <downloaded-release-assets> --native-smoke-dir <native-smoke-win32-artifact-dir> --native-smoke-dir <native-smoke-darwin-artifact-dir> --native-smoke-dir <native-smoke-linux-artifact-dir> --install-smoke-dir <install-smoke-evidence-dir> --updater-smoke-dir <updater-smoke-evidence-dir> --accessibility-smoke-dir <accessibility-smoke-evidence-dir>`, adding `--signed`, `--require-attestations`, `--require-desktop-target`, `--require-virtual-audio`, `--require-app-insertion-drills`, `--require-benchmarks`, or `--require-branch-protection` when those release gates apply.

## Accessibility Smoke

- Windows: run NVDA through onboarding, settings navigation, recording, cancellation, paste failure recovery, and history review.
- macOS: run VoiceOver through onboarding, settings navigation, recording, cancellation, paste failure recovery, and history review.
- Linux: run Orca through onboarding, settings navigation, recording, cancellation, paste failure recovery, and history review.
- Confirm keyboard-only navigation reaches every settings section and every visible recovery action.
- Confirm recording, processing, inserted, copied, cancelled, and paste-failed states are announced without exposing transcript text unexpectedly.
- Confirm Arabic and Hebrew UI smoke paths preserve readable mixed-direction labels, buttons, and recovery text.
- Retain `accessibility-smoke*.json` for Windows, macOS, and Linux, then run `bun run check:accessibility-smoke-release-evidence -- --dir <accessibility-smoke-evidence-dir>`.

## Android Release (APK/AAB)

Release Android builds must serve the bundled frontend. `BuildTask.kt` enforces
this for the Rust library: any `--release` cargo build automatically adds the
`tauri/custom-protocol` feature and nulls out `build.devUrl` via the
`TAURI_CONFIG` merge patch, so the dev-server URL (`http://localhost:1420`)
never reaches a release artifact. Do not bypass Gradle/`tauri android build`
with a hand-rolled cargo invocation for release artifacts.

Build recipe:

1. Bump `bundle.android.versionCode` in `src-tauri/tauri.android.conf.json`.
   Version codes are consumed forever once uploaded to a Play draft, even a
   discarded one (11000 is burned; shipped fixes start at 11001).
2. Set release signing environment variables before building:
   `VERBATIM_ANDROID_KEYSTORE_FILE`, `VERBATIM_ANDROID_KEYSTORE_PASSWORD`,
   `VERBATIM_ANDROID_KEY_ALIAS`, and optionally `VERBATIM_ANDROID_KEY_PASSWORD`
   (defaults to the keystore password). Without them the release build falls
   back to unsigned output.
3. Build: `bun run tauri android build` (AAB for Play) and
   `bun run tauri android build --apk` (sideload APK).
4. Verify every artifact before distributing:
   `bun run check:android-release-bundle <path-to-apk-or-aab> [...]`.
   This fails if `libverbatim_app_lib.so` contains `localhost:1420` or lacks
   the embedded frontend HTML document.
5. Run the artifact on a device or emulator and confirm the app UI loads
   (not a browser error page).

## Do Not Publish If

- Any expected asset is missing.
- Any updater signature is missing.
- Any installer is unsigned when the release is intended to be signed.
- Any signed-release CI signing verification step is skipped or failed.
- Windows SmartScreen, macOS Gatekeeper, or Linux package installation behavior differs from the release notes.
- The release body does not distinguish desktop downloads from any other platform downloads.
- Any Android artifact fails `bun run check:android-release-bundle`.
