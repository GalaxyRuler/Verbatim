#[cfg(all(feature = "android-asr", target_os = "android"))]
fn main() -> anyhow::Result<()> {
    android::run()
}

#[cfg(not(all(feature = "android-asr", target_os = "android")))]
fn main() {
    eprintln!(
        "asr-sensevoice-smoke is only available for Android builds with --features android-asr"
    );
    std::process::exit(2);
}

#[cfg(all(feature = "android-asr", target_os = "android"))]
mod android {
    use anyhow::{Context, Result};
    use serde::Deserialize;
    use std::env;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::Instant;
    use verbatim_app_lib::asr::offline::OfflineRecognizer;
    use verbatim_app_lib::asr::AsrModelPaths;

    pub fn run() -> Result<()> {
        let args = Args::parse()?;
        let entries = read_manifest(&args.manifest)?;
        let manifest_dir = args
            .manifest
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let paths = AsrModelPaths::for_dir(&args.model_dir);
        let started = Instant::now();
        let mut recognizer = OfflineRecognizer::new_sense_voice(&paths)
            .context("failed to create SenseVoice recognizer")?;
        eprintln!(
            "recognizer_init_ms={:.1} {}",
            started.elapsed().as_secs_f64() * 1000.0,
            memory_fields()
        );

        for entry in entries {
            let wav_path = resolve_wav_path(&entry.wav, args.corpus_root.as_deref(), &manifest_dir);
            let wave = sherpa_onnx::Wave::read(wav_path.to_string_lossy().as_ref())
                .with_context(|| format!("failed to read wav {}", wav_path.display()))?;
            if wave.sample_rate() != 16_000 {
                anyhow::bail!(
                    "{} sample rate {} != 16000",
                    wav_path.display(),
                    wave.sample_rate()
                );
            }
            let decode_started = Instant::now();
            let text = recognizer
                .transcribe(wave.sample_rate(), wave.samples())
                .with_context(|| format!("failed to transcribe {}", wav_path.display()))?;
            let latency_ms = decode_started.elapsed().as_secs_f64() * 1000.0;
            println!(
                "language={} wav={} samples={} latency_ms={:.1} {} transcript={}",
                entry.language,
                wav_path.display(),
                wave.samples().len(),
                latency_ms,
                memory_fields(),
                text.replace('\n', " ")
            );
        }

        Ok(())
    }

    #[derive(Debug)]
    struct Args {
        model_dir: PathBuf,
        manifest: PathBuf,
        corpus_root: Option<PathBuf>,
    }

    impl Args {
        fn parse() -> Result<Self> {
            let mut model_dir = None;
            let mut manifest = None;
            let mut corpus_root = None;
            let mut args = env::args().skip(1);
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--model-dir" => {
                        model_dir = Some(PathBuf::from(next_value(&mut args, "--model-dir")?))
                    }
                    "--manifest" => {
                        manifest = Some(PathBuf::from(next_value(&mut args, "--manifest")?))
                    }
                    "--corpus-root" => {
                        corpus_root = Some(PathBuf::from(next_value(&mut args, "--corpus-root")?))
                    }
                    "-h" | "--help" => {
                        eprintln!("Usage: asr-sensevoice-smoke --model-dir DIR --manifest FILE [--corpus-root DIR]");
                        std::process::exit(0);
                    }
                    _ => anyhow::bail!("unknown argument {arg:?}; pass --help for usage"),
                }
            }
            Ok(Self {
                model_dir: model_dir.context("--model-dir is required")?,
                manifest: manifest.context("--manifest is required")?,
                corpus_root,
            })
        }
    }

    fn next_value(args: &mut impl Iterator<Item = String>, name: &str) -> Result<String> {
        args.next()
            .with_context(|| format!("{name} requires a value"))
    }

    #[derive(Debug, Deserialize)]
    struct ManifestEntry {
        language: String,
        wav: String,
    }

    fn read_manifest(path: &Path) -> Result<Vec<ManifestEntry>> {
        let value = serde_json::from_str::<serde_json::Value>(&fs::read_to_string(path)?)?;
        let entries = value
            .get("entries")
            .cloned()
            .with_context(|| format!("manifest {} must contain entries", path.display()))?;
        serde_json::from_value(entries)
            .with_context(|| format!("failed to decode entries in {}", path.display()))
    }

    fn resolve_wav_path(wav: &str, corpus_root: Option<&Path>, manifest_dir: &Path) -> PathBuf {
        let wav_path = PathBuf::from(wav);
        if wav_path.is_absolute() {
            wav_path
        } else if let Some(root) = corpus_root {
            root.join(wav_path)
        } else {
            manifest_dir.join(wav_path)
        }
    }

    fn memory_fields() -> String {
        match read_status_memory() {
            Ok(memory) => format!(
                "vm_rss_kb={} vm_hwm_kb={}",
                memory
                    .vm_rss_kb
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                memory
                    .vm_hwm_kb
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            ),
            Err(error) => format!("vm_rss_kb=unknown vm_hwm_kb=unknown memory_error={error}"),
        }
    }

    fn read_status_memory() -> Result<MemoryStatus> {
        let status = fs::read_to_string("/proc/self/status")?;
        let mut memory = MemoryStatus::default();
        for line in status.lines() {
            if let Some(value) = parse_status_kb(line, "VmRSS:") {
                memory.vm_rss_kb = Some(value);
            } else if let Some(value) = parse_status_kb(line, "VmHWM:") {
                memory.vm_hwm_kb = Some(value);
            }
        }
        Ok(memory)
    }

    fn parse_status_kb(line: &str, label: &str) -> Option<u64> {
        line.strip_prefix(label)?
            .split_whitespace()
            .next()?
            .parse()
            .ok()
    }

    #[derive(Default)]
    struct MemoryStatus {
        vm_rss_kb: Option<u64>,
        vm_hwm_kb: Option<u64>,
    }
}
