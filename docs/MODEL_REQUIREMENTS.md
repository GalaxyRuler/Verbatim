# Model Requirements

Every built-in downloadable transcription model must be represented in
`src-tauri/resources/model_catalog.json` with enough metadata to make the
download, runtime, and release risk reviewable before it reaches users.

Required fields:

- `url`: HTTPS URL or `asset:` indirection that resolves to the Verbatim asset
  host.
- `sha256`: 64-character hexadecimal SHA-256 for the exact downloadable
  artifact.
- `sizeMb`: positive expected artifact size in MB.
- `licenseLabel`: upstream model or conversion license label. Use `Requires
upstream review` only as a temporary blocker label when the source artifact is
  bundled but the upstream model card is not yet linked or verified.
- `acceleratorSupport`: runtime accelerator family used by the model. Current
  accepted values are `whisper-cpp` for Whisper GGML models and `onnx-runtime`
  for transcribe-rs ONNX-backed engines.
- `supportedLanguages`: explicit language codes or a named language set.
- `supportsLanguageSelection`: whether user language selection is honored by
  the engine.

Custom user-provided models are allowed, but they must stay marked unverified:
no download URL, no SHA-256, `is_custom = true`, and a UI label that makes clear
Verbatim has not verified the file, license, checksum, or behavior.

Do not publish numeric hardware or throughput recommendations without reviewed
benchmark evidence. Use [BENCHMARKS.md](BENCHMARKS.md) for the required result
format and validation gate.
