# Native Smoke Tests

This directory is reserved for packaged desktop smoke fixtures and controlled native targets.

Current coverage lives in:

- `.github/workflows/native-smoke.yml`
- `scripts/native-smoke/run-packaged-smoke.ts`
- `scripts/native-smoke/controlled-desktop-targets.ts`
- `scripts/native-smoke/virtual-audio-input.ts`
- `scripts/native-smoke/run-whiteknight-golden-dictation.ps1`

The current smoke lane proves packaged startup, settings load, main-window creation, tray initialization, updater plugin registration, single-instance plugin registration, close-to-tray handler registration, clean quit, isolated profile use, clean-profile retention defaults and empty history/recordings state, production storage-policy behavior for disabled recordings, disabled history, and private session, credential-store health without retained legacy API keys, synthetic legacy API-key migration into the real OS credential backend when that backend is available, model-load fallback decision behavior for accelerator and generic provider failures, insertion safety for target-changed adaptive/classic paste attempts, clipboard ownership safety for same-text and changed-text mutation cases, production frontend main/overlay assets plus lazy locale chunks, packaged app resolution/decoding of critical bundled resources, deterministic WAV fixture generation, forced startup-failure recovery status, forced coordinator panic supervision status, single-instance duplicate exit, smoke-only local model selection without remote provider calls, installer/package launch from the produced NSIS/DMG/DEB artifact, Windows generated-uninstaller cleanup, Ubuntu package removal cleanup, and artifact capture.

Virtual-audio lanes can pass `--smoke-microphone <name>` to `bun run
smoke:native`, or set `VERBATIM_SMOKE_SELECTED_MICROPHONE`, after creating the
OS virtual input device. The packaged app applies that selection before the
recording manager initializes and reports it in status JSON, so the runner can
fail if the app is still using the wrong microphone.

## Local inference receipt

An opt-in packaged smoke can prove that a real local model loads and produces a
non-empty result from a deterministic 16 kHz mono PCM WAV. The receipt records
only model ID, sample count, booleans, and failure class; it never stores the
dictated audio or transcript. The runner also supplies an artifact-local data
directory, including on Windows where setting `APPDATA` alone does not isolate
known-folder application storage.

```bash
bun run smoke:native -- \
  --app path/to/Verbatim \
  --artifact-dir native-smoke-artifacts \
  --real-inference-wav fixtures/synthetic-speech-16khz-mono.wav \
  --real-inference-model moonshine-tiny-streaming-en \
  --real-inference-model-dir fixtures/models \
  --require-real-inference

bun run check:native-smoke-artifacts -- \
  --dir native-smoke-artifacts \
  --require-real-inference
```

The model directory must be disposable; it is used as the smoke model root and
can receive the model's local integrity inventory. This verifies the local ASR
boundary, but it does not substitute for a virtual-microphone capture test.

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

## Golden dictation evidence contract

The desktop release gate is only allowed to claim the golden dictation path
after a fresh packaged build, running with an isolated profile, proves all of
the following in one controlled session:

1. The selected virtual microphone is the input used by the packaged app.
2. A deterministic WAV fixture is played through that input.
3. The app starts and completes local model inference from that recording.
4. A focus change during inference blocks insertion into the replacement text
   target.
5. A synthetic clipboard mutation after Verbatim's clipboard write is left
   intact; the evidence records booleans and reason codes only, never clipboard
   contents or dictated text.

Startup, unit-level insertion, and controlled-target preflight evidence are
useful prerequisites, but none can substitute for this contract. The physical
lane is deliberately opt-in: it uses the dedicated WhiteKnight desktop runner,
never the developer workstation or a user's installed Verbatim app. It stages
a fresh build, a local model, and the deterministic fixture under a unique
`C:\AgentArtifacts\whiteknight-tasks\<run>` directory, then deletes that exact
remote task directory after copying only redacted evidence back to the
controller.

Before invocation, the WhiteKnight readiness and interactive-desktop gates must
be `Ready`. Its reversible desktop QA mode must be disabled after the run.
The physical runner must already have the exact VB-Audio endpoints available;
the lane does not install drivers, alter default audio devices, use a live
microphone, or read pre-existing clipboard contents.

```powershell
# Run from the controller after building a fresh app payload.
bun run smoke:golden-dictation -- `
  -AppPath C:\build\verbatim.exe `
  -ModelDirectory C:\fixtures\models `
  -WavPath C:\fixtures\synthetic-speech-16khz-mono.wav `
  -ModelId moonshine-tiny-streaming-en `
  -InputDeviceName 'CABLE Output (VB-Audio Virtual Cable)' `
  -OutputDeviceName 'Speakers (VB-Audio Virtual Cable)'
```

The command fails closed if any `verbatim.exe` is already running on
WhiteKnight, if either requested virtual endpoint is absent, if the staged app
is not the selected model/microphone, or if any of the three real desktop
cases fails. The runner retains no dictated audio, transcript, or user
clipboard value. Its only copied evidence is fixed-schema JSON/JSONL receipts
containing booleans, device/model identity, insertion reason codes, and frame
counts.

To independently validate a copied evidence directory without requiring the
normal packaged-smoke artifacts, run:

```bash
bun run check:native-smoke-artifacts -- \
  --dir C:\CodexScratch\verbatim-golden-dictation\<run>\evidence \
  --require-golden-dictation \
  --golden-dictation-only
```

This gate permits only the documented fixed-schema fields, so it rejects an
artifact that contains transcript, audio, or clipboard-content fields in
addition to rejecting missing or failed stable-focus, focus-switch, and
clipboard-mutation receipts.

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

Do not use live microphones, paid providers, or user clipboard contents in these tests.
