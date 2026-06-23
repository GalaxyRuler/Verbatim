# Native Smoke Tests

This directory is reserved for packaged desktop smoke fixtures and controlled native targets.

Current coverage lives in:

- `.github/workflows/native-smoke.yml`
- `scripts/native-smoke/run-packaged-smoke.ts`
- `scripts/native-smoke/controlled-desktop-targets.ts`
- `scripts/native-smoke/virtual-audio-input.ts`

The current smoke lane proves packaged startup, settings load, settings schema/domain version metadata, main-window creation, tray initialization, updater plugin registration, single-instance plugin registration, close-to-tray handler registration, clean quit, isolated profile use, clean-profile retention defaults and empty history/recordings state, production storage-policy behavior for disabled recordings, disabled history, and private session, credential-store health without retained legacy API keys, synthetic legacy API-key migration into the real OS credential backend when that backend is available, model-load fallback decision behavior for accelerator and generic provider failures, insertion safety for target-changed adaptive/classic paste attempts, clipboard ownership safety for same-text and changed-text mutation cases, production frontend main/overlay assets plus lazy locale chunks, packaged app resolution/decoding of critical bundled resources, deterministic WAV fixture generation, forced startup-failure recovery status, forced coordinator panic supervision status, single-instance duplicate exit, smoke-only local model selection without remote provider calls, installer/package launch from the produced NSIS/DMG/DEB artifact, Windows generated-uninstaller cleanup, Ubuntu package removal cleanup, and artifact capture.

Virtual-audio lanes can pass `--smoke-microphone <name>` to `bun run
smoke:native`, or set `VERBATIM_SMOKE_SELECTED_MICROPHONE`, after creating the
OS virtual input device. The packaged app applies that selection before the
recording manager initializes and reports it in status JSON, so the runner can
fail if the app is still using the wrong microphone.

Use `bun run smoke:virtual-audio` to record virtual-input provisioning evidence.
On Linux, `--create-linux-pulse-source` creates a PulseAudio/PipeWire-compatible
source named `verbatim_smoke_source` and reports `pactl unload-module` cleanup
commands. On Windows and macOS, install a virtual audio device in the disposable
desktop session and pass its input name with `--device-name`.
Use `--cleanup-from native-smoke-artifacts/virtual-audio-input.json` to unload
Linux modules and write `virtual-audio-cleanup.json`.
Use `--input-from native-smoke-artifacts/virtual-audio-input.json
--play-fixture <path>` to play a generated WAV into the Linux virtual sink and
write `virtual-audio-playback.json`.

The GitHub `native smoke` workflow has matching opt-in inputs:
`virtual-audio-preflight` enables the helper, and `virtual-audio-device-name`
is required on Windows/macOS. Linux provisions `verbatim_smoke_source`
automatically when the input is enabled, exports it for packaged and installer
smoke, plays the generated fixture into the virtual sink after packaged smoke,
and runs cleanup in an `always()` step.

Before artifact upload, the workflow runs `bun run check:native-smoke-artifacts`
against `native-smoke-artifacts` with `--require-installer`. It adds
`--require-desktop-target` or `--require-virtual-audio` when those opt-in lanes
are enabled, preventing a green workflow from silently omitting required
retained evidence.

Full app-driven insertion race evidence is a separate contract from the
controlled-target preflight. When an automation lane can make the packaged app
perform the insertion attempt, write `app-insertion-drills.json` with
`schema_version: 1` and cases named
`focus_switch_during_inference_blocks_insertion` and
`clipboard_mutation_during_paste_preserves_user_clipboard`, then run:

```bash
bun run smoke:native -- --app-insertion-drills native-smoke-artifacts/app-insertion-drills.json --require-app-insertion-drills
```

The runner fails if either case is missing, not app-driven, not passed, lacks a
desktop target, records user clipboard contents, or does not prove the expected
focus-switch/clipboard-mutation condition. Do not use this gate with the
controlled-target preflight artifact alone.

Controlled desktop target coverage is opt-in:

```bash
bun run smoke:desktop-targets -- --artifact-dir native-smoke-artifacts --require
bun run smoke:native -- --desktop-target-drill --require-desktop-target --allow-text-entry
```

The controlled-target drill launches a disposable OS text target where available:
Notepad on Windows, TextEdit on macOS, and `gedit`, `mousepad`, or `xterm` on
Linux. Text entry and clipboard mutation are disabled by default. CI jobs that
run in isolated desktop sessions can add `--allow-text-entry` to type a
synthetic marker into the disposable target and verify it was saved, plus
`--allow-clipboard-write` to write and read back a synthetic clipboard marker.
The drill never reads user clipboard contents.

Remaining fixture work:

- OS virtual audio input path for packaged transcription smoke.
- Real inference smoke that records and transcribes the deterministic audio
  fixture from the provisioned virtual input.
- N-1 to N updater application smoke.
- Real provider load-failure execution that proves the native engine can recover
  after an actual accelerator load failure.
- Runtime zero-retention and private-session dictation drills with real audio
  and model inference that prove actual dictation creates no retained history
  row or WAV file when storage is disabled.
- OS credential-store migration evidence on CI platforms where the credential
  backend is unavailable or cannot persist the synthetic smoke key.
- Full packaged webview CSP violation drill beyond the static config check and
  resource/locale asset smoke.
- Wire the controlled paste targets into full Verbatim insertion smoke so the
  app performs the paste into the target, not only the target launch/focus
  preflight.
- Focus-switch during inference with insertion blocked against a real desktop
  target.
- Clipboard mutation during paste with user clipboard preserved in an isolated
  desktop session.

Do not use live microphones, paid providers, or user clipboard contents in these tests.
