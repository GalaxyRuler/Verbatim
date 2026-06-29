#[cfg(any(test, all(feature = "android-asr", target_os = "android")))]
#[path = "../asr/wer.rs"]
mod wer;

#[cfg(all(feature = "android-asr", target_os = "android"))]
fn main() -> anyhow::Result<()> {
    android::run()
}

#[cfg(not(all(feature = "android-asr", target_os = "android")))]
fn main() {
    eprintln!("asr-wer is only available for Android builds with --features android-asr");
    std::process::exit(2);
}

#[cfg(all(feature = "android-asr", target_os = "android"))]
mod android {
    use crate::wer::{aggregate_word_error_rates, word_error_rate, WordErrorRate};
    use anyhow::{Context, Result};
    use serde::{Deserialize, Serialize};
    use std::env;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};
    use verbatim_app_lib::asr::offline::OfflineRecognizer;
    use verbatim_app_lib::asr::streaming::StreamingRecognizer;
    use verbatim_app_lib::asr::AsrModelPaths;

    const DEFAULT_LANGUAGE: &str = "en";
    const DEFAULT_CHUNK_MS: u64 = 100;

    pub fn run() -> Result<()> {
        let args = Args::parse()?;
        let entries = read_manifest(&args.manifest)?;
        if entries.is_empty() {
            anyhow::bail!("manifest {} has no entries", args.manifest.display());
        }

        let paths = AsrModelPaths::for_dir(&args.model_dir);
        let mut offline = OfflineRecognizer::new(&paths, &args.language)
            .context("failed to initialize offline Whisper recognizer")?;

        let manifest_dir = args
            .manifest
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let mut file_reports = Vec::with_capacity(entries.len());

        for entry in entries {
            let wav_path = resolve_wav_path(&entry.wav, args.corpus_root.as_deref(), &manifest_dir);
            let wave = read_wave(&wav_path)?;
            if wave.sample_rate != 16_000 {
                anyhow::bail!(
                    "{} has sample rate {}; expected 16000 Hz",
                    wav_path.display(),
                    wave.sample_rate
                );
            }

            let streaming_result =
                run_streaming(&paths, wave.sample_rate, &wave.samples, args.chunk_ms);
            let streaming = streaming_result.with_context(|| {
                format!("streaming recognizer failed for {}", wav_path.display())
            })?;

            let offline_started = Instant::now();
            let offline_hypothesis = offline
                .transcribe(wave.sample_rate, &wave.samples)
                .with_context(|| format!("offline recognizer failed for {}", wav_path.display()))?;
            let offline_latency_ms = elapsed_ms(offline_started.elapsed());

            let streaming_wer = word_error_rate(&entry.reference, &streaming.hypothesis);
            let offline_wer = word_error_rate(&entry.reference, &offline_hypothesis);
            let total_latency_ms = streaming.total_latency_ms + offline_latency_ms;
            let audio_duration_ms =
                (wave.samples.len() as f64 / f64::from(wave.sample_rate)) * 1000.0;

            file_reports.push(FileReport {
                id: entry.id.unwrap_or_else(|| entry.wav.clone()),
                wav: wav_path.to_string_lossy().into_owned(),
                reference: entry.reference,
                streaming_hypothesis: streaming.hypothesis,
                offline_hypothesis,
                first_partial: streaming.first_partial,
                streaming_first_partial_latency_ms: streaming.first_partial_latency_ms,
                streaming_total_latency_ms: streaming.total_latency_ms,
                offline_latency_ms,
                total_latency_ms,
                audio_duration_ms,
                sample_rate: wave.sample_rate,
                sample_count: wave.samples.len(),
                streaming_wer,
                offline_wer,
            });
        }

        let aggregate = AggregateReport::from_files(&file_reports);
        let report = AsrWerReport {
            model_dir: args.model_dir.to_string_lossy().into_owned(),
            manifest: args.manifest.to_string_lossy().into_owned(),
            language: args.language,
            chunk_ms: args.chunk_ms,
            files: file_reports,
            aggregate,
        };

        let summary = human_summary(&report);
        eprint!("{summary}");

        let json = serde_json::to_string_pretty(&report)?;
        if let Some(json_out) = args.json_out {
            fs::write(&json_out, &json)
                .with_context(|| format!("failed to write {}", json_out.display()))?;
        }
        println!("{json}");

        Ok(())
    }

    #[derive(Debug)]
    struct Args {
        model_dir: PathBuf,
        manifest: PathBuf,
        corpus_root: Option<PathBuf>,
        language: String,
        chunk_ms: u64,
        json_out: Option<PathBuf>,
    }

    impl Args {
        fn parse() -> Result<Self> {
            let mut model_dir = None;
            let mut manifest = None;
            let mut corpus_root = None;
            let mut language = DEFAULT_LANGUAGE.to_string();
            let mut chunk_ms = DEFAULT_CHUNK_MS;
            let mut json_out = None;
            let mut args = env::args().skip(1);

            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "-h" | "--help" => {
                        print_usage();
                        std::process::exit(0);
                    }
                    "--model-dir" => model_dir = Some(next_path(&mut args, "--model-dir")?),
                    "--manifest" => manifest = Some(next_path(&mut args, "--manifest")?),
                    "--corpus-root" => corpus_root = Some(next_path(&mut args, "--corpus-root")?),
                    "--language" => language = next_value(&mut args, "--language")?,
                    "--chunk-ms" => {
                        chunk_ms = next_value(&mut args, "--chunk-ms")?
                            .parse()
                            .context("--chunk-ms must be a positive integer")?
                    }
                    "--json-out" => json_out = Some(next_path(&mut args, "--json-out")?),
                    _ => anyhow::bail!("unknown argument {arg:?}; pass --help for usage"),
                }
            }

            if chunk_ms == 0 {
                anyhow::bail!("--chunk-ms must be greater than zero");
            }

            Ok(Self {
                model_dir: model_dir.context("--model-dir is required")?,
                manifest: manifest.context("--manifest is required")?,
                corpus_root,
                language,
                chunk_ms,
                json_out,
            })
        }
    }

    fn next_path(args: &mut impl Iterator<Item = String>, name: &str) -> Result<PathBuf> {
        Ok(PathBuf::from(next_value(args, name)?))
    }

    fn next_value(args: &mut impl Iterator<Item = String>, name: &str) -> Result<String> {
        args.next()
            .with_context(|| format!("{name} requires a value"))
    }

    fn print_usage() {
        eprintln!(
            "Usage: asr-wer --model-dir DIR --manifest FILE [--corpus-root DIR] [--language en] [--chunk-ms 100] [--json-out FILE]"
        );
        eprintln!("stdout: JSON report");
        eprintln!("stderr: human summary");
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ManifestEntry {
        #[serde(default)]
        id: Option<String>,
        wav: String,
        reference: String,
    }

    fn read_manifest(path: &Path) -> Result<Vec<ManifestEntry>> {
        let value = serde_json::from_str::<serde_json::Value>(
            &fs::read_to_string(path)
                .with_context(|| format!("failed to read manifest {}", path.display()))?,
        )
        .with_context(|| format!("failed to parse manifest {}", path.display()))?;

        if value.is_array() {
            return serde_json::from_value(value).with_context(|| {
                format!("failed to decode manifest entries in {}", path.display())
            });
        }

        let entries = value
            .get("entries")
            .or_else(|| value.get("items"))
            .cloned()
            .with_context(|| {
                format!(
                    "manifest {} must be an array or an object with entries/items",
                    path.display()
                )
            })?;

        serde_json::from_value(entries)
            .with_context(|| format!("failed to decode manifest entries in {}", path.display()))
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

    struct WaveData {
        sample_rate: i32,
        samples: Vec<f32>,
    }

    fn read_wave(path: &Path) -> Result<WaveData> {
        let path_string = path.to_string_lossy();
        let wave = sherpa_onnx::Wave::read(path_string.as_ref())
            .with_context(|| format!("failed to read wav {}", path.display()))?;

        Ok(WaveData {
            sample_rate: wave.sample_rate(),
            samples: wave.samples().to_vec(),
        })
    }

    struct StreamingRun {
        hypothesis: String,
        first_partial: Option<String>,
        first_partial_latency_ms: Option<f64>,
        total_latency_ms: f64,
    }

    fn run_streaming(
        paths: &AsrModelPaths,
        sample_rate: i32,
        samples: &[f32],
        chunk_ms: u64,
    ) -> Result<StreamingRun> {
        let mut recognizer = StreamingRecognizer::new(paths)?;
        let chunk_size = ((sample_rate as u64 * chunk_ms) / 1000).max(1) as usize;
        let started = Instant::now();
        let mut first_partial = None;
        let mut first_partial_latency_ms = None;
        let mut last_partial = String::new();

        for chunk in samples.chunks(chunk_size) {
            if recognizer.accept_waveform(sample_rate, chunk)? {
                let partial = recognizer.partial_text()?;
                if partial.trim().is_empty() {
                    continue;
                }
                if first_partial.is_none() {
                    first_partial_latency_ms = Some(elapsed_ms(started.elapsed()));
                    first_partial = Some(partial.clone());
                }
                last_partial = partial;
            }
        }

        let mut hypothesis = recognizer.finish()?;
        if hypothesis.trim().is_empty() {
            hypothesis = last_partial;
        }

        Ok(StreamingRun {
            hypothesis,
            first_partial,
            first_partial_latency_ms,
            total_latency_ms: elapsed_ms(started.elapsed()),
        })
    }

    fn elapsed_ms(duration: Duration) -> f64 {
        duration.as_secs_f64() * 1000.0
    }

    #[derive(Debug, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct AsrWerReport {
        model_dir: String,
        manifest: String,
        language: String,
        chunk_ms: u64,
        files: Vec<FileReport>,
        aggregate: AggregateReport,
    }

    #[derive(Debug, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct FileReport {
        id: String,
        wav: String,
        reference: String,
        streaming_hypothesis: String,
        offline_hypothesis: String,
        first_partial: Option<String>,
        streaming_first_partial_latency_ms: Option<f64>,
        streaming_total_latency_ms: f64,
        offline_latency_ms: f64,
        total_latency_ms: f64,
        audio_duration_ms: f64,
        sample_rate: i32,
        sample_count: usize,
        streaming_wer: WordErrorRate,
        offline_wer: WordErrorRate,
    }

    #[derive(Debug, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct AggregateReport {
        file_count: usize,
        streaming_wer: WordErrorRate,
        offline_wer: WordErrorRate,
        avg_streaming_first_partial_latency_ms: Option<f64>,
        avg_streaming_total_latency_ms: f64,
        avg_offline_latency_ms: f64,
        avg_total_latency_ms: f64,
    }

    impl AggregateReport {
        fn from_files(files: &[FileReport]) -> Self {
            let streaming_wers = files
                .iter()
                .map(|file| file.streaming_wer.clone())
                .collect::<Vec<_>>();
            let offline_wers = files
                .iter()
                .map(|file| file.offline_wer.clone())
                .collect::<Vec<_>>();
            let first_partials = files
                .iter()
                .filter_map(|file| file.streaming_first_partial_latency_ms)
                .collect::<Vec<_>>();

            Self {
                file_count: files.len(),
                streaming_wer: aggregate_word_error_rates(&streaming_wers),
                offline_wer: aggregate_word_error_rates(&offline_wers),
                avg_streaming_first_partial_latency_ms: average(&first_partials),
                avg_streaming_total_latency_ms: average_by(files, |file| {
                    file.streaming_total_latency_ms
                }),
                avg_offline_latency_ms: average_by(files, |file| file.offline_latency_ms),
                avg_total_latency_ms: average_by(files, |file| file.total_latency_ms),
            }
        }
    }

    fn average(values: &[f64]) -> Option<f64> {
        (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
    }

    fn average_by(files: &[FileReport], read: impl Fn(&FileReport) -> f64) -> f64 {
        if files.is_empty() {
            return 0.0;
        }

        files.iter().map(read).sum::<f64>() / files.len() as f64
    }

    fn human_summary(report: &AsrWerReport) -> String {
        let mut summary = String::new();
        summary.push_str("asr-wer summary\n");
        summary.push_str(&format!("files: {}\n", report.aggregate.file_count));
        summary.push_str(&format!(
            "offline WER: {:.2}% ({} errors / {} reference words)\n",
            report.aggregate.offline_wer.wer * 100.0,
            report.aggregate.offline_wer.errors,
            report.aggregate.offline_wer.reference_words
        ));
        summary.push_str(&format!(
            "streaming WER: {:.2}% ({} errors / {} reference words)\n",
            report.aggregate.streaming_wer.wer * 100.0,
            report.aggregate.streaming_wer.errors,
            report.aggregate.streaming_wer.reference_words
        ));
        if let Some(first_partial_ms) = report.aggregate.avg_streaming_first_partial_latency_ms {
            summary.push_str(&format!("avg first partial: {:.1} ms\n", first_partial_ms));
        } else {
            summary.push_str("avg first partial: none\n");
        }
        summary.push_str(&format!(
            "avg total latency: {:.1} ms\n",
            report.aggregate.avg_total_latency_ms
        ));

        for file in &report.files {
            summary.push_str(&format!(
                "\n{}: offline {:.2}% streaming {:.2}% first_partial {} total {:.1} ms\n",
                file.id,
                file.offline_wer.wer * 100.0,
                file.streaming_wer.wer * 100.0,
                format_optional_ms(file.streaming_first_partial_latency_ms),
                file.total_latency_ms
            ));
            summary.push_str(&format!("  ref: {}\n", file.reference));
            summary.push_str(&format!("  off: {}\n", file.offline_hypothesis.trim()));
            summary.push_str(&format!("  str: {}\n", file.streaming_hypothesis.trim()));
        }

        summary
    }

    fn format_optional_ms(value: Option<f64>) -> String {
        value
            .map(|latency| format!("{latency:.1} ms"))
            .unwrap_or_else(|| "none".to_string())
    }
}
