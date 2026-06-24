import { existsSync, readdirSync, readFileSync } from "node:fs";
import path from "node:path";

type AccessibilitySmokeEvidence = {
  schema_version?: number;
  platform?: string;
  assistive_technology?: string;
  tester?: string;
  tested_at?: string;
  version?: string;
  onboarding_verified?: boolean;
  settings_navigation_verified?: boolean;
  recording_verified?: boolean;
  cancellation_verified?: boolean;
  paste_failure_recovery_verified?: boolean;
  history_review_verified?: boolean;
  keyboard_only_navigation_verified?: boolean;
  live_states_announced_without_transcript_leak?: boolean;
  rtl_mixed_direction_verified?: boolean;
  failures?: string[];
};

const args = process.argv.slice(2);

if (args.includes("--help") || args.includes("-h")) {
  console.log(`Usage: bun scripts/check-accessibility-smoke-evidence.ts [options]

Options:
  --dir <path>                       Directory containing accessibility smoke evidence JSON.
  --require-platform <platform>      Require evidence for one platform. Repeatable.
  --require-representative-platforms Require windows, macos, and linux evidence.
  --help                             Show this help text.
`);
  process.exit(0);
}

const evidenceDir = path.resolve(
  argValue("--dir") ?? "dist/accessibility-smoke-evidence",
);
const requiredPlatforms = new Set(argsValues("--require-platform"));
if (hasArg("--require-representative-platforms")) {
  requiredPlatforms.add("windows");
  requiredPlatforms.add("macos");
  requiredPlatforms.add("linux");
}

const expectedAssistiveTechnology = new Map([
  ["windows", "NVDA"],
  ["macos", "VoiceOver"],
  ["linux", "Orca"],
]);
const failures: string[] = [];
const evidenceFiles = listEvidenceFiles();
const evidenceByPlatform = new Map<string, AccessibilitySmokeEvidence>();

for (const fileName of evidenceFiles) {
  const evidence = readEvidence(fileName);
  if (!evidence) continue;
  validateEvidence(fileName, evidence);
  if (typeof evidence.platform === "string") {
    evidenceByPlatform.set(evidence.platform, evidence);
  }
}

for (const platform of requiredPlatforms) {
  if (!evidenceByPlatform.has(platform)) {
    failures.push(`Missing accessibility smoke evidence for ${platform}.`);
  }
}

if (failures.length > 0) {
  console.error("Accessibility smoke evidence check failed:");
  for (const failure of failures) {
    console.error(`- ${failure}`);
  }
  process.exit(1);
}

console.log(
  `Accessibility smoke evidence check passed for ${evidenceByPlatform.size} platform(s).`,
);

function argValue(name: string): string | undefined {
  const index = args.indexOf(name);
  if (index >= 0) return args[index + 1];
  const prefix = `${name}=`;
  return args.find((arg) => arg.startsWith(prefix))?.slice(prefix.length);
}

function argsValues(name: string): string[] {
  const values: string[] = [];
  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    if (arg === name && args[index + 1]) {
      values.push(args[index + 1]);
      index += 1;
      continue;
    }
    const prefix = `${name}=`;
    if (arg.startsWith(prefix)) values.push(arg.slice(prefix.length));
  }
  return values;
}

function hasArg(name: string): boolean {
  return args.includes(name);
}

function listEvidenceFiles(): string[] {
  if (!existsSync(evidenceDir)) {
    failures.push(`${evidenceDir} does not exist.`);
    return [];
  }

  const files = readdirSync(evidenceDir)
    .filter((name) => /^accessibility-smoke.*\.json$/i.test(name))
    .sort();

  if (files.length === 0) {
    failures.push(
      `${evidenceDir} contains no accessibility-smoke*.json files.`,
    );
  }
  return files;
}

function readEvidence(fileName: string): AccessibilitySmokeEvidence | null {
  const filePath = path.join(evidenceDir, fileName);
  try {
    return JSON.parse(
      readFileSync(filePath, "utf8"),
    ) as AccessibilitySmokeEvidence;
  } catch (error) {
    failures.push(`${fileName} is not valid JSON: ${error}`);
    return null;
  }
}

function validateEvidence(
  fileName: string,
  evidence: AccessibilitySmokeEvidence,
): void {
  if (evidence.schema_version !== 1) {
    failures.push(`${fileName} schema_version must be 1.`);
  }
  if (!["windows", "macos", "linux"].includes(evidence.platform ?? "")) {
    failures.push(`${fileName} platform must be windows, macos, or linux.`);
  }
  if (!isVersion(evidence.version)) {
    failures.push(`${fileName} version must be a semantic version.`);
  }
  if (
    typeof evidence.tester !== "string" ||
    evidence.tester.trim().length === 0
  ) {
    failures.push(`${fileName} tester must be present.`);
  }
  if (
    typeof evidence.tested_at !== "string" ||
    Number.isNaN(Date.parse(evidence.tested_at))
  ) {
    failures.push(`${fileName} tested_at must be an ISO date.`);
  }

  const expectedAt = expectedAssistiveTechnology.get(evidence.platform ?? "");
  if (expectedAt && evidence.assistive_technology !== expectedAt) {
    failures.push(
      `${fileName} assistive_technology must be ${expectedAt} for ${evidence.platform}.`,
    );
  }

  requireTrue(fileName, evidence.onboarding_verified, "onboarding_verified");
  requireTrue(
    fileName,
    evidence.settings_navigation_verified,
    "settings_navigation_verified",
  );
  requireTrue(fileName, evidence.recording_verified, "recording_verified");
  requireTrue(
    fileName,
    evidence.cancellation_verified,
    "cancellation_verified",
  );
  requireTrue(
    fileName,
    evidence.paste_failure_recovery_verified,
    "paste_failure_recovery_verified",
  );
  requireTrue(
    fileName,
    evidence.history_review_verified,
    "history_review_verified",
  );
  requireTrue(
    fileName,
    evidence.keyboard_only_navigation_verified,
    "keyboard_only_navigation_verified",
  );
  requireTrue(
    fileName,
    evidence.live_states_announced_without_transcript_leak,
    "live_states_announced_without_transcript_leak",
  );
  requireTrue(
    fileName,
    evidence.rtl_mixed_direction_verified,
    "rtl_mixed_direction_verified",
  );

  for (const failure of evidence.failures ?? []) {
    failures.push(`${fileName} recorded failure: ${failure}`);
  }
}

function requireTrue(
  fileName: string,
  value: boolean | undefined,
  field: string,
): void {
  if (value !== true) failures.push(`${fileName} ${field} must be true.`);
}

function isVersion(value: string | undefined): value is string {
  return (
    typeof value === "string" &&
    /^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(value)
  );
}
