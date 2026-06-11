# Verbatim CDN Assets

Verbatim-owned release builds and model downloads should use:

```text
https://cdn.galaxyruler.com/verbatim
```

The base URL is configurable in GitHub Actions with the `VERBATIM_ASSET_BASE_URL`
repository variable and in Rust builds with the compile-time
`VERBATIM_ASSET_BASE_URL` environment variable.

Required mirrored files:

- `silero_vad_v4.onnx`
- `whisper-medium-q4_1.bin`
- `breeze-asr-q5_k.bin`
- `parakeet-v2-int8.tar.gz`
- `parakeet-v3-int8.tar.gz`
- `moonshine-base.tar.gz`
- `moonshine-tiny-streaming-en.tar.gz`
- `moonshine-small-streaming-en.tar.gz`
- `moonshine-medium-streaming-en.tar.gz`
- `sense-voice-int8.tar.gz`
- `giga-am-v3-int8.tar.gz`
- `canary-180m-flash.tar.gz`
- `canary-1b-v2.tar.gz`
- `cohere-int8.tar.gz`
- `onnxruntime-osx-x86_64-1.24.2.tgz`

Linux ONNX Runtime builds use Microsoft release assets directly. The macOS
x86_64 ONNX Runtime archive must be mirrored because Microsoft does not publish
that file for 1.24.2.
