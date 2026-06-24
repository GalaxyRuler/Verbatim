# Troubleshooting

## The installer or app shows a security warning

The builds are currently unsigned.

- **Windows:** SmartScreen → **More info → Run anyway**.
- **macOS:** right-click the app → **Open** (only needed the first time).

## No text appears when I dictate

- Make sure a text field is focused — the cursor must be where you want the text.
- Check that a model is downloaded and selected (see [Models](models.md)).
- Make sure your hotkey doesn't clash with another app's shortcut.

## My microphone isn't picked up

Grant microphone permission to Verbatim:

- **macOS:** System Settings → Privacy & Security → Microphone → enable Verbatim.
- **Windows:** Settings → Privacy & security → Microphone → allow desktop apps to access the microphone.
- **Linux:** make sure the right input device is selected and not muted (PulseAudio / PipeWire).

Then confirm the correct input device is chosen in Verbatim's settings.

## Transcription is slow or inaccurate

- For speed, switch to a smaller / quantized model; for accuracy, switch to a larger one (see [Models](models.md)).
- The first run after launch is slower while the model loads into memory.

## Still stuck?

[Open an issue](https://github.com/GalaxyRuler/Verbatim/issues) and include your OS, the Verbatim version, and what you've already tried.
