import {
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { execSync } from "node:child_process";
import os from "node:os";
import path from "node:path";

type BenchmarkRun = {
  durationMs: number;
  audioSeconds: number;
  realTimeFactor: number;
};

type BenchmarkResult = {
  schemaVersion: 1;
  generatedAt: string;
  appVersion: string;
  gitRevision: string;
  platform: {
    os: string;
    arch: string;
    cpu: string;
    gpu: string | null;
    memoryGb: number;
  };
  model: {
    id: string;
    engine: string;
    accelerator: string;
  };
  fixture: {
    id: string;
    audioSeconds: number;
    sampleRateHz: number;
  };
  runs: BenchmarkRun[];
};

const benchmarkDir = path.join("benchmarks", "model-performance");
const command = process.argv[2];
const args = process.argv.slice(3);

if (!command || command === "--help" || command === "-h") {
  printHelp();
  process.exit(0);
}

if (command === "record") {
  recordBenchmark();
} else if (command === "recommend") {
  recommendFromLocalResults();
} else {
  fail(`Unknown command: ${command}`);
}

function recordBenchmark(): void {
  const modelId = requiredArg("--model-id");
  const engine = requiredArg("--engine");
  const accelerator = requiredArg("--accelerator");
  const fixtureId = requiredArg("--fixture-id");
  const audioSeconds = positiveNumberArg("--audio-seconds");
  const sampleRateHz = positiveNumberArg("--sample-rate-hz");
  const durationMs = parseDurations(requiredArg("--duration-ms"));

  if (durationMs.length < 3) {
    fail(
      "--duration-ms must provide at least three positive comma-separated run durations.",
    );
  }

  const result: BenchmarkResult = {
    schemaVersion: 1,
    generatedAt: new Date().toISOString(),
    appVersion: argValue("--app-version") ?? packageVersion(),
    gitRevision: argValue("--git-revision") ?? gitRevision(),
    platform: {
      os: argValue("--os") ?? os.platform(),
      arch: argValue("--arch") ?? os.arch(),
      cpu: argValue("--cpu") ?? cpuModel(),
      gpu: argValue("--gpu") ?? null,
      memoryGb: numberArg("--memory-gb") ?? memoryGb(),
    },
    model: {
      id: modelId,
      engine,
      accelerator,
    },
    fixture: {
      id: fixtureId,
      audioSeconds,
      sampleRateHz,
    },
    runs: durationMs.map((duration) => ({
      durationMs: duration,
      audioSeconds,
      realTimeFactor: round(audioSeconds / (duration / 1000), 3),
    })),
  };

  const outputPath =
    argValue("--out") ??
    path.join(
      benchmarkDir,
      `${safeSlug(result.generatedAt)}-${safeSlug(modelId)}-${safeSlug(accelerator)}.json`,
    );
  mkdirSync(path.dirname(outputPath), { recursive: true });
  writeFileSync(outputPath, `${JSON.stringify(result, null, 2)}\n`, "utf8");

  console.log(`Wrote local benchmark result: ${outputPath}`);
  printResultSummary(result);
}

function recommendFromLocalResults(): void {
  const files = listBenchmarkFiles(argValue("--dir") ?? benchmarkDir);
  if (files.length === 0) {
    console.log(
      "No local benchmark result files found. Run benchmark:model:record before asking for a local recommendation.",
    );
    return;
  }

  const results = files
    .map((file) => readBenchmarkResult(file))
    .filter(
      (result): result is BenchmarkResult & { __file: string } =>
        result !== null,
    );
  const ranked = results
    .map((result) => ({
      label: `${result.model.id} (${result.model.engine}, ${result.model.accelerator})`,
      medianRealTimeFactor: median(
        result.runs.map((run) => run.realTimeFactor),
      ),
      file: result.__file,
    }))
    .sort(
      (left, right) => right.medianRealTimeFactor - left.medianRealTimeFactor,
    );

  if (ranked.length === 0) {
    fail("No valid local benchmark results found.");
  }

  console.log("Local-only model ranking from benchmark files:");
  for (const [index, item] of ranked.entries()) {
    console.log(
      `${index + 1}. ${item.label}: median ${round(item.medianRealTimeFactor, 3)}x real-time (${item.file})`,
    );
  }
  console.log(
    "Do not publish this as a public hardware recommendation until reviewed benchmark evidence is committed.",
  );
}

function readBenchmarkResult(
  filePath: string,
): (BenchmarkResult & { __file: string }) | null {
  try {
    const parsed = JSON.parse(
      readFileSync(filePath, "utf8"),
    ) as BenchmarkResult;
    if (parsed.schemaVersion !== 1 || !Array.isArray(parsed.runs)) {
      return null;
    }
    return { ...parsed, __file: filePath };
  } catch {
    return null;
  }
}

function listBenchmarkFiles(root: string): string[] {
  if (!existsSync(root)) return [];
  const files: string[] = [];
  const walk = (current: string) => {
    for (const entry of readdirSync(current)) {
      const fullPath = path.join(current, entry);
      const stat = statSync(fullPath);
      if (stat.isDirectory()) {
        walk(fullPath);
      } else if (entry.endsWith(".json")) {
        files.push(fullPath);
      }
    }
  };
  walk(root);
  return files.sort();
}

function requiredArg(name: string): string {
  const value = argValue(name);
  if (!value) fail(`${name} is required.`);
  return value;
}

function argValue(name: string): string | undefined {
  const index = args.indexOf(name);
  if (index >= 0) return args[index + 1];
  const prefix = `${name}=`;
  return args.find((arg) => arg.startsWith(prefix))?.slice(prefix.length);
}

function positiveNumberArg(name: string): number {
  const value = numberArg(name);
  if (value === undefined || value <= 0)
    fail(`${name} must be a positive number.`);
  return value;
}

function numberArg(name: string): number | undefined {
  const value = argValue(name);
  if (value === undefined) return undefined;
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) fail(`${name} must be numeric.`);
  return parsed;
}

function parseDurations(value: string): number[] {
  return value.split(/[,\s]+/).map((duration) => {
    const parsed = Number(duration.trim());
    if (!Number.isFinite(parsed) || parsed <= 0) {
      fail("--duration-ms must contain only positive numeric run durations.");
    }
    return parsed;
  });
}

function packageVersion(): string {
  const packageJson = JSON.parse(readFileSync("package.json", "utf8")) as {
    version?: string;
  };
  return packageJson.version ?? "unknown";
}

function gitRevision(): string {
  try {
    return execSync("git rev-parse HEAD", { encoding: "utf8" }).trim();
  } catch {
    return "unknown";
  }
}

function cpuModel(): string {
  return os.cpus()[0]?.model ?? "unknown";
}

function memoryGb(): number {
  return round(os.totalmem() / 1024 ** 3, 1);
}

function median(values: number[]): number {
  const sorted = [...values].sort((a, b) => a - b);
  const midpoint = Math.floor(sorted.length / 2);
  if (sorted.length % 2 === 1) return sorted[midpoint];
  return (sorted[midpoint - 1] + sorted[midpoint]) / 2;
}

function round(value: number, places: number): number {
  const factor = 10 ** places;
  return Math.round(value * factor) / factor;
}

function safeSlug(value: string): string {
  return value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "");
}

function printResultSummary(result: BenchmarkResult): void {
  console.log(
    `${result.model.id} (${result.model.engine}, ${result.model.accelerator}) median: ${round(
      median(result.runs.map((run) => run.realTimeFactor)),
      3,
    )}x real-time`,
  );
}

function printHelp(): void {
  console.log(`Usage:
  bun run benchmark:model:record -- --model-id <id> --engine <engine> --accelerator <accelerator> --fixture-id <id> --audio-seconds <n> --sample-rate-hz <n> --duration-ms=<ms,ms,ms>
  bun run benchmark:model:recommend

This is a local-only helper. It records manually measured benchmark timings and
prints recommendations from local JSON files without telemetry or network calls.`);
}

function fail(message: string): never {
  console.error(message);
  process.exit(1);
}
