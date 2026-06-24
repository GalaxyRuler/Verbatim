import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";

type BenchmarkResult = {
  schemaVersion?: number;
  generatedAt?: string;
  appVersion?: string;
  gitRevision?: string;
  platform?: {
    os?: string;
    arch?: string;
    cpu?: string;
    gpu?: string | null;
    memoryGb?: number;
  };
  model?: {
    id?: string;
    engine?: string;
    accelerator?: string;
  };
  fixture?: {
    id?: string;
    audioSeconds?: number;
    sampleRateHz?: number;
  };
  runs?: Array<{
    durationMs?: number;
    audioSeconds?: number;
    realTimeFactor?: number;
  }>;
};

const benchmarkDir = path.join("benchmarks", "model-performance");
const docsToScan = ["README.md", "docs/MODEL_REQUIREMENTS.md"];
const failures: string[] = [];
const requireRepresentativePlatforms = process.argv
  .slice(2)
  .includes("--require-representative-platforms");

for (const docPath of docsToScan) {
  if (!existsSync(docPath)) continue;
  const text = readFileSync(docPath, "utf8");
  const unsupportedPatterns = [
    /~?\d+(?:\.\d+)?x\s+real[- ]time/gi,
    /\btested on\s+(?:i\d|m\d|ryzen|geforce|radeon|rtx|gtx|apple silicon)\b/gi,
  ];

  for (const pattern of unsupportedPatterns) {
    for (const match of text.matchAll(pattern)) {
      failures.push(
        `${docPath} contains unsupported benchmark claim '${match[0]}'. Add structured benchmark evidence before publishing numeric model-performance guidance.`,
      );
    }
  }
}

const benchmarkFiles = listBenchmarkFiles(benchmarkDir);
const benchmarkResults: BenchmarkResult[] = [];
for (const filePath of benchmarkFiles) {
  const result = validateBenchmarkFile(filePath);
  if (result) benchmarkResults.push(result);
}

if (requireRepresentativePlatforms) {
  validateRepresentativePlatforms(benchmarkResults);
}

if (failures.length > 0) {
  console.error("Model benchmark evidence check failed:");
  for (const failure of failures) {
    console.error(`- ${failure}`);
  }
  process.exit(1);
}

if (benchmarkFiles.length === 0) {
  console.log(
    "No model benchmark result files found; public docs contain no unsupported numeric throughput claims.",
  );
} else {
  console.log(
    `Model benchmark evidence check passed for ${benchmarkFiles.length} result file(s).`,
  );
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

function validateBenchmarkFile(filePath: string): BenchmarkResult | null {
  let result: BenchmarkResult;
  try {
    result = JSON.parse(readFileSync(filePath, "utf8")) as BenchmarkResult;
  } catch (error) {
    failures.push(`${filePath} is not valid JSON: ${error}`);
    return null;
  }

  requireString(filePath, result.generatedAt, "generatedAt");
  requireString(filePath, result.appVersion, "appVersion");
  requireString(filePath, result.gitRevision, "gitRevision");
  requireString(filePath, result.platform?.os, "platform.os");
  requireString(filePath, result.platform?.arch, "platform.arch");
  requireString(filePath, result.platform?.cpu, "platform.cpu");
  requirePositiveNumber(
    filePath,
    result.platform?.memoryGb,
    "platform.memoryGb",
  );
  requireString(filePath, result.model?.id, "model.id");
  requireString(filePath, result.model?.engine, "model.engine");
  requireString(filePath, result.model?.accelerator, "model.accelerator");
  requireString(filePath, result.fixture?.id, "fixture.id");
  requirePositiveNumber(
    filePath,
    result.fixture?.audioSeconds,
    "fixture.audioSeconds",
  );
  requirePositiveNumber(
    filePath,
    result.fixture?.sampleRateHz,
    "fixture.sampleRateHz",
  );

  if (result.schemaVersion !== 1) {
    failures.push(`${filePath} schemaVersion must be 1.`);
  }

  if (!Array.isArray(result.runs) || result.runs.length < 3) {
    failures.push(`${filePath} must include at least three benchmark runs.`);
    return result;
  }

  for (const [index, run] of result.runs.entries()) {
    const prefix = `runs[${index}]`;
    requirePositiveNumber(filePath, run.durationMs, `${prefix}.durationMs`);
    requirePositiveNumber(filePath, run.audioSeconds, `${prefix}.audioSeconds`);
    requirePositiveNumber(
      filePath,
      run.realTimeFactor,
      `${prefix}.realTimeFactor`,
    );

    if (
      typeof run.durationMs === "number" &&
      typeof run.audioSeconds === "number" &&
      typeof run.realTimeFactor === "number"
    ) {
      const expected = run.audioSeconds / (run.durationMs / 1000);
      const drift = Math.abs(expected - run.realTimeFactor);
      if (drift > Math.max(0.05, expected * 0.05)) {
        failures.push(
          `${filePath} ${prefix}.realTimeFactor=${run.realTimeFactor} does not match duration/audio (${expected.toFixed(3)}).`,
        );
      }
    }
  }

  return result;
}

function validateRepresentativePlatforms(results: BenchmarkResult[]): void {
  const platformNames = new Set(
    results
      .map((result) => normalizePlatform(result.platform?.os))
      .filter((platform): platform is string => Boolean(platform)),
  );
  const requiredPlatforms = ["windows", "macos", "linux"];
  const missingPlatforms = requiredPlatforms.filter(
    (platform) => !platformNames.has(platform),
  );

  if (missingPlatforms.length > 0) {
    failures.push(
      `Representative benchmark evidence is missing for: ${missingPlatforms.join(", ")}. Run reviewed benchmark captures on Windows, macOS, and Linux before publishing hardware recommendations.`,
    );
  }
}

function normalizePlatform(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const normalized = value.trim().toLowerCase();
  if (["windows", "win32"].includes(normalized)) return "windows";
  if (["macos", "mac", "darwin"].includes(normalized)) return "macos";
  if (["linux"].includes(normalized)) return "linux";
  return normalized || null;
}

function requireString(filePath: string, value: unknown, field: string): void {
  if (typeof value !== "string" || value.trim() === "") {
    failures.push(`${filePath} ${field} must be a non-empty string.`);
  }
}

function requirePositiveNumber(
  filePath: string,
  value: unknown,
  field: string,
): void {
  if (typeof value !== "number" || !Number.isFinite(value) || value <= 0) {
    failures.push(`${filePath} ${field} must be a positive number.`);
  }
}
