# Verbatim CDN Assets

Verbatim-owned release builds and model downloads should use:

```text
https://verbatim-assets.galaxyruler.space
```

The base URL is configurable in GitHub Actions with the `VERBATIM_ASSET_BASE_URL`
repository variable and in Rust builds with the compile-time
`VERBATIM_ASSET_BASE_URL` environment variable. The current production target is
the Cloudflare R2 custom domain above.

Compatibility fallback:

```text
https://galaxyruler.space/verbatim-assets
```

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
- `cohere-arabic-int8.tar.gz`
- `onnxruntime-osx-x86_64-1.24.2.tgz`

Linux ONNX Runtime builds use Microsoft release assets directly. The macOS
x86_64 ONNX Runtime archive must be mirrored because Microsoft does not publish
that file for 1.24.2.

## R2 bucket

Production files are mirrored to the Cloudflare R2 bucket `verbatim-assets`.
The URL for each file should be:

```text
https://verbatim-assets.galaxyruler.space/<filename>
```

After syncing assets, verify the host with:

```bash
curl -I https://verbatim-assets.galaxyruler.space/silero_vad_v4.onnx
curl -I https://verbatim-assets.galaxyruler.space/whisper-medium-q4_1.bin
```

## VPS compatibility route

The `galaxyruler.space` VPS can also serve the same files from:

```text
/var/www/galaxyruler.space/verbatim-assets/
```

Compatibility URL:

```text
https://galaxyruler.space/verbatim-assets/<filename>
```

Current Caddy route:

```caddyfile
handle /verbatim-assets/* {
    root * /var/www/galaxyruler.space
    header Cache-Control "public, max-age=31536000, immutable"
    file_server
}
```

Verify the fallback route with:

```bash
curl -I https://galaxyruler.space/verbatim-assets/silero_vad_v4.onnx
curl -I https://galaxyruler.space/verbatim-assets/whisper-medium-q4_1.bin
```

The response must be the real asset, not the website fallback. Treat
`Content-Type: text/html` as a failed deployment even if the HTTP status is
`200`.
