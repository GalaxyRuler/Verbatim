# Benchmark Evidence

Do not publish numeric model-performance recommendations until they are backed
by repeatable local benchmark results. Benchmark runs must not use telemetry,
live microphones, paid providers, private transcripts, or user clipboard data.

Store reviewed result files under `benchmarks/model-performance/` as JSON. The
repository does not need to include large audio fixtures; result files should
identify the fixture and record enough machine context for reviewers to compare
runs.

## Local Profiling Helper

Use the local helper when manually profiling a deterministic fixture on a
developer machine:

```bash
bun run benchmark:model:record -- --model-id parakeet-v3 --engine onnx-runtime --accelerator cpu --fixture-id deterministic-speech-fixture-v1 --audio-seconds 30 --sample-rate-hz 16000 --duration-ms=6000,6100,5950
```

The helper only records timings that were measured elsewhere; it does not open
a microphone, read private transcripts, call paid providers, upload telemetry,
or make network requests. It writes structured JSON to
`benchmarks/model-performance/` unless `--out <path>` is supplied.

To compare local result files on the same machine:

```bash
bun run benchmark:model:recommend
```

The recommendation command reads local JSON files and ranks model/engine/
accelerator combinations by median real-time factor. Treat that output as a
local machine recommendation only. Public hardware guidance still requires
reviewed benchmark evidence across representative Windows, macOS, and Linux
systems.

Required result shape:

```json
{
  "schemaVersion": 1,
  "generatedAt": "2026-06-22T00:00:00.000Z",
  "appVersion": "0.8.8",
  "gitRevision": "0000000000000000000000000000000000000000",
  "platform": {
    "os": "windows",
    "arch": "x64",
    "cpu": "CPU model",
    "gpu": "GPU model or null",
    "memoryGb": 16
  },
  "model": {
    "id": "parakeet-v3",
    "engine": "onnx-runtime",
    "accelerator": "cpu"
  },
  "fixture": {
    "id": "deterministic-speech-fixture-v1",
    "audioSeconds": 30,
    "sampleRateHz": 16000
  },
  "runs": [
    {
      "durationMs": 6000,
      "audioSeconds": 30,
      "realTimeFactor": 5
    },
    {
      "durationMs": 6100,
      "audioSeconds": 30,
      "realTimeFactor": 4.918
    },
    {
      "durationMs": 5950,
      "audioSeconds": 30,
      "realTimeFactor": 5.042
    }
  ]
}
```

Run the evidence gate before publishing model-performance guidance:

```bash
bun run check:model-benchmark-evidence
```

The gate validates structured result files when present and rejects public docs
that contain unsupported numeric throughput claims such as `5x real-time` or
hardware-specific claims without benchmark evidence.

Before publishing public hardware recommendations, run the stricter release
gate:

```bash
bun run check:model-benchmark-release-evidence
```

That command requires reviewed benchmark result files from representative
Windows, macOS, and Linux systems. It is expected to fail until all three
platforms have structured benchmark evidence under `benchmarks/model-performance/`.
