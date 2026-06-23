import { existsSync, readdirSync, readFileSync } from "node:fs";
import path from "node:path";

type UpdaterSmokeEvidence = {
  schema_version?: number;
  platform?: string;
  previous_version?: string;
  target_version?: string;
  previous_install_verified?: boolean;
  update_detected?: boolean;
  update_downloaded?: boolean;
  updater_signature_verified?: boolean;
  update_applied?: boolean;
  relaunched_version?: string;
  clean_profile_preserved?: boolean;
  latest_json_url?: string;
  updater_archive_name?: string;
  failures?: string[];
};

const args = process.argv.slice(2);

if (args.includes("--help") || args.includes("-h")) {
  console.log(`Usage: bun scripts/check-updater-smoke-evidence.ts [options]

Options:
  --dir <path>                       Directory containing updater smoke evidence JSON.
  --require-platform <platform>      Require evidence for one platform. Repeatable.
  --require-representative-platforms Require windows-x86_64, darwin-aarch64, and linux-x86_64.
  --help                             Show this help text.
`);
  process.exit(0);
}

const evidenceDir = path.resolve(
  argValue("--dir") ?? "dist/updater-smoke-evidence",
);
const requiredPlatforms = new Set(argsValues("--require-platform"));
if (hasArg("--require-representative-platforms")) {
  requiredPlatforms.add("windows-x86_64");
  requiredPlatforms.add("darwin-aarch64");
  requiredPlatforms.add("linux-x86_64");
}

const failures: string[] = [];
const evidenceFiles = listEvidenceFiles();
const evidenceByPlatform = new Map<string, UpdaterSmokeEvidence>();

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
    failures.push(`Missing updater smoke evidence for ${platform}.`);
  }
}

if (failures.length > 0) {
  console.error("Updater smoke evidence check failed:");
  for (const failure of failures) {
    console.error(`- ${failure}`);
  }
  process.exit(1);
}

console.log(
  `Updater smoke evidence check passed for ${evidenceByPlatform.size} platform(s).`,
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
    .filter((name) => /^updater-smoke.*\.json$/i.test(name))
    .sort();

  if (files.length === 0) {
    failures.push(`${evidenceDir} contains no updater-smoke*.json files.`);
  }
  return files;
}

function readEvidence(fileName: string): UpdaterSmokeEvidence | null {
  const filePath = path.join(evidenceDir, fileName);
  try {
    return JSON.parse(readFileSync(filePath, "utf8")) as UpdaterSmokeEvidence;
  } catch (error) {
    failures.push(`${fileName} is not valid JSON: ${error}`);
    return null;
  }
}

function validateEvidence(
  fileName: string,
  evidence: UpdaterSmokeEvidence,
): void {
  if (evidence.schema_version !== 1) {
    failures.push(`${fileName} schema_version must be 1.`);
  }
  if (
    !["windows-x86_64", "darwin-aarch64", "linux-x86_64"].includes(
      evidence.platform ?? "",
    )
  ) {
    failures.push(
      `${fileName} platform must be windows-x86_64, darwin-aarch64, or linux-x86_64.`,
    );
  }
  if (!isVersion(evidence.previous_version)) {
    failures.push(`${fileName} previous_version must be a semantic version.`);
  }
  if (!isVersion(evidence.target_version)) {
    failures.push(`${fileName} target_version must be a semantic version.`);
  }
  if (
    isVersion(evidence.previous_version) &&
    isVersion(evidence.target_version) &&
    evidence.previous_version === evidence.target_version
  ) {
    failures.push(
      `${fileName} previous_version and target_version must differ.`,
    );
  }
  requireTrue(
    fileName,
    evidence.previous_install_verified,
    "previous_install_verified",
  );
  requireTrue(fileName, evidence.update_detected, "update_detected");
  requireTrue(fileName, evidence.update_downloaded, "update_downloaded");
  requireTrue(
    fileName,
    evidence.updater_signature_verified,
    "updater_signature_verified",
  );
  requireTrue(fileName, evidence.update_applied, "update_applied");
  requireTrue(
    fileName,
    evidence.clean_profile_preserved,
    "clean_profile_preserved",
  );

  if (evidence.relaunched_version !== evidence.target_version) {
    failures.push(`${fileName} relaunched_version must match target_version.`);
  }
  if (!isHttpsUrl(evidence.latest_json_url)) {
    failures.push(`${fileName} latest_json_url must be an HTTPS URL.`);
  }
  if (
    typeof evidence.updater_archive_name !== "string" ||
    evidence.updater_archive_name.trim() === ""
  ) {
    failures.push(`${fileName} updater_archive_name must be present.`);
  }
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

function isHttpsUrl(value: string | undefined): value is string {
  if (typeof value !== "string") return false;
  try {
    return new URL(value).protocol === "https:";
  } catch {
    return false;
  }
}
