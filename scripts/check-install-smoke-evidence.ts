import { existsSync, readdirSync, readFileSync } from "node:fs";
import path from "node:path";

type InstallSmokeEvidence = {
  schema_version?: number;
  platform?: string;
  version?: string;
  artifact_name?: string;
  clean_machine?: boolean;
  install_verified?: boolean;
  launch_verified?: boolean;
  local_transcription_verified?: boolean;
  plain_text_insertion_verified?: boolean;
  uninstall_verified?: boolean;
  app_removed_after_uninstall?: boolean;
  app_data_policy_checked?: boolean;
  windows_default_uninstall_preserved_app_data?: boolean | null;
  windows_delete_app_data_removed_app_data?: boolean | null;
  macos_gatekeeper_verified?: boolean | null;
  linux_package_manager_verified?: boolean | null;
  trust_behavior_matches_release_notes?: boolean;
  failures?: string[];
};

const args = process.argv.slice(2);

if (args.includes("--help") || args.includes("-h")) {
  console.log(`Usage: bun scripts/check-install-smoke-evidence.ts [options]

Options:
  --dir <path>                       Directory containing install smoke evidence JSON.
  --require-platform <platform>      Require evidence for one platform. Repeatable.
  --require-representative-platforms Require windows-x86_64, darwin-aarch64, and linux-x86_64.
  --signed                           Require signed-release trust evidence.
  --help                             Show this help text.
`);
  process.exit(0);
}

const evidenceDir = path.resolve(
  argValue("--dir") ?? "dist/install-smoke-evidence",
);
const requireSigned = hasArg("--signed");
const requiredPlatforms = new Set(argsValues("--require-platform"));
if (hasArg("--require-representative-platforms")) {
  requiredPlatforms.add("windows-x86_64");
  requiredPlatforms.add("darwin-aarch64");
  requiredPlatforms.add("linux-x86_64");
}

const failures: string[] = [];
const evidenceFiles = listEvidenceFiles();
const evidenceByPlatform = new Map<string, InstallSmokeEvidence>();

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
    failures.push(`Missing install smoke evidence for ${platform}.`);
  }
}

if (failures.length > 0) {
  console.error("Install smoke evidence check failed:");
  for (const failure of failures) {
    console.error(`- ${failure}`);
  }
  process.exit(1);
}

console.log(
  `Install smoke evidence check passed for ${evidenceByPlatform.size} platform(s).`,
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
    .filter((name) => /^install-smoke.*\.json$/i.test(name))
    .sort();

  if (files.length === 0) {
    failures.push(`${evidenceDir} contains no install-smoke*.json files.`);
  }
  return files;
}

function readEvidence(fileName: string): InstallSmokeEvidence | null {
  const filePath = path.join(evidenceDir, fileName);
  try {
    return JSON.parse(readFileSync(filePath, "utf8")) as InstallSmokeEvidence;
  } catch (error) {
    failures.push(`${fileName} is not valid JSON: ${error}`);
    return null;
  }
}

function validateEvidence(
  fileName: string,
  evidence: InstallSmokeEvidence,
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
  if (!isVersion(evidence.version)) {
    failures.push(`${fileName} version must be a semantic version.`);
  }
  if (
    typeof evidence.artifact_name !== "string" ||
    evidence.artifact_name.trim() === ""
  ) {
    failures.push(`${fileName} artifact_name must be present.`);
  }

  requireTrue(fileName, evidence.clean_machine, "clean_machine");
  requireTrue(fileName, evidence.install_verified, "install_verified");
  requireTrue(fileName, evidence.launch_verified, "launch_verified");
  requireTrue(
    fileName,
    evidence.local_transcription_verified,
    "local_transcription_verified",
  );
  requireTrue(
    fileName,
    evidence.plain_text_insertion_verified,
    "plain_text_insertion_verified",
  );
  requireTrue(fileName, evidence.uninstall_verified, "uninstall_verified");
  requireTrue(
    fileName,
    evidence.app_removed_after_uninstall,
    "app_removed_after_uninstall",
  );
  requireTrue(
    fileName,
    evidence.app_data_policy_checked,
    "app_data_policy_checked",
  );
  requireTrue(
    fileName,
    evidence.trust_behavior_matches_release_notes,
    "trust_behavior_matches_release_notes",
  );

  if (evidence.platform === "windows-x86_64") {
    requireTrue(
      fileName,
      evidence.windows_default_uninstall_preserved_app_data === true,
      "windows_default_uninstall_preserved_app_data",
    );
    requireTrue(
      fileName,
      evidence.windows_delete_app_data_removed_app_data === true,
      "windows_delete_app_data_removed_app_data",
    );
  }

  if (requireSigned && evidence.platform === "darwin-aarch64") {
    requireTrue(
      fileName,
      evidence.macos_gatekeeper_verified === true,
      "macos_gatekeeper_verified",
    );
  }

  if (evidence.platform === "linux-x86_64") {
    requireTrue(
      fileName,
      evidence.linux_package_manager_verified === true,
      "linux_package_manager_verified",
    );
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
