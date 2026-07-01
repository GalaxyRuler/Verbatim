# Models

Verbatim transcribes using open speech-recognition models that run entirely on your machine. You can switch models at any time to balance speed and accuracy.

## Supported families

Verbatim works with several open model families, including:

- **Whisper** (including distilled / quantized variants)
- **Parakeet**
- **Moonshine**
- and others

The exact models and sizes available depend on your version — see the model picker in Settings.

## Choosing a model

- **Smaller / quantized models** — fastest, lowest memory use, great for quick dictation and modest hardware.
- **Larger models** — higher accuracy at the cost of more memory and a little more latency.

If transcription feels slow, switch to a smaller model. If accuracy matters more than speed, switch to a larger one.

On Android, larger on-device ASR packs may be hidden behind RAM gates to prevent unstable inference. Parakeet TDT 0.6B v2 requires at least 12 GB RAM because the 652 MB encoder can use more than 2 GB during inference.

## Downloading and storage

Models are downloaded once and cached on disk, so you don't re-download them. After a model is cached, Verbatim works fully offline.
