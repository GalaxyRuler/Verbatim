import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import path from "node:path";

type Check = {
  label: string;
  script: string;
  args: string[];
};

const args = process.argv.slice(2);

if (args.includes("--help") || args.includes("-h")) {
  console.log(`Usage: bun scripts/check-release-readiness-evidence.ts [options]

Options:
  --release-assets-dir <path>        Directory containing RELEASE_MANIFEST.json, SHA256SUMS.txt, latest.json, and assets.
  --native-smoke-dir <path>          Native smoke artifact directory. Repeat for each platform artifact.
  --install-smoke-dir <path>         Directory containing install-smoke*.json evidence.
  --updater-smoke-dir <path>         Directory containing updater-smoke*.json evidence.
  --accessibility-smoke-dir <path>   Directory containing accessibility-smoke*.json evidence.
  --signed                           Require signed release evidence.
  --require-attestations             Require retained gh attestation verification evidence for packaged desktop assets.
  --attestations-dir <path>          Directory containing <asset>.attestation.json files. Defaults to <release-assets-dir>/attestations.
  --require-desktop-target           Require desktop-target evidence in each native smoke directory.
  --require-virtual-audio            Require virtual-audio evidence in each native smoke directory.
  --require-app-insertion-drills     Require app-driven insertion evidence in each native smoke directory.
  --require-benchmarks               Require representative Windows/macOS/Linux benchmark evidence.
  --require-branch-protection        Require branch protection native backend contexts.
  --branch-protection-repo <repo>     Repository for branch protection check. Defaults to GalaxyRuler/Verbatim.
  --branch-protection-branch <branch> Branch for branch protection check. Defaults to main.
  --help                             Show this help text.
`);
  process.exit(0);
}

const releaseAssetsDir =
  argValue("--release-assets-dir") ?? "dist/release-evidence";
const nativeSmokeDirs = argsValues("--native-smoke-dir");
const installSmokeDir =
  argValue("--install-smoke-dir") ?? "dist/install-smoke-evidence";
const updaterSmokeDir =
  argValue("--updater-smoke-dir") ?? "dist/updater-smoke-evidence";
const accessibilitySmokeDir =
  argValue("--accessibility-smoke-dir") ?? "dist/accessibility-smoke-evidence";
const signed = hasArg("--signed");
const requireAttestations = hasArg("--require-attestations");
const attestationsDir = argValue("--attestations-dir");
const requireDesktopTarget = hasArg("--require-desktop-target");
const requireVirtualAudio = hasArg("--require-virtual-audio");
const requireAppInsertionDrills = hasArg("--require-app-insertion-drills");
const requireBenchmarks = hasArg("--require-benchmarks");
const requireBranchProtection = hasArg("--require-branch-protection");
const branchProtectionRepo =
  argValue("--branch-protection-repo") ?? "GalaxyRuler/Verbatim";
const branchProtectionBranch = argValue("--branch-protection-branch") ?? "main";
const requiredNativeSmokePlatforms = ["win32", "darwin", "linux"];
const failures: string[] = [];

if (nativeSmokeDirs.length === 0) {
  failures.push("At least one --native-smoke-dir is required.");
}

const nativeSmokePlatforms = new Map<string, string>();
for (const dir of nativeSmokeDirs) {
  const platform = readNativeSmokePlatform(dir);
  if (platform) {
    if (nativeSmokePlatforms.has(platform)) {
      failures.push(
        `Duplicate native smoke evidence for ${platform}: ${nativeSmokePlatforms.get(platform)} and ${dir}.`,
      );
    }
    nativeSmokePlatforms.set(platform, dir);
  }
}

for (const platform of requiredNativeSmokePlatforms) {
  if (!nativeSmokePlatforms.has(platform)) {
    failures.push(`Missing native smoke evidence for ${platform}.`);
  }
}

const checks: Check[] = [
  {
    label: signed ? "signed release asset evidence" : "release asset evidence",
    script: "scripts/check-release-evidence.ts",
    args: [
      ...(signed ? ["--signed"] : []),
      ...(requireAttestations ? ["--require-attestations"] : []),
      ...(attestationsDir ? ["--attestations-dir", attestationsDir] : []),
      "--dir",
      releaseAssetsDir,
    ],
  },
  ...nativeSmokeDirs.map((dir, index): Check => {
    const nativeArgs = ["--dir", dir, "--require-installer"];
    const platform = [...nativeSmokePlatforms.entries()].find(
      ([, platformDir]) => platformDir === dir,
    )?.[0];
    if (platform) nativeArgs.push("--require-platform", platform);
    if (requireDesktopTarget) nativeArgs.push("--require-desktop-target");
    if (requireVirtualAudio) nativeArgs.push("--require-virtual-audio");
    if (requireAppInsertionDrills) {
      nativeArgs.push("--require-app-insertion-drills");
    }
    return {
      label: `native smoke evidence ${index + 1}`,
      script: "scripts/native-smoke/check-artifacts.ts",
      args: nativeArgs,
    };
  }),
  {
    label: signed ? "signed install smoke evidence" : "install smoke evidence",
    script: "scripts/check-install-smoke-evidence.ts",
    args: [
      "--require-representative-platforms",
      ...(signed ? ["--signed"] : []),
      "--dir",
      installSmokeDir,
    ],
  },
  {
    label: "updater smoke evidence",
    script: "scripts/check-updater-smoke-evidence.ts",
    args: ["--require-representative-platforms", "--dir", updaterSmokeDir],
  },
  {
    label: "accessibility smoke evidence",
    script: "scripts/check-accessibility-smoke-evidence.ts",
    args: [
      "--require-representative-platforms",
      "--dir",
      accessibilitySmokeDir,
    ],
  },
];

if (requireBenchmarks) {
  checks.push({
    label: "representative model benchmark evidence",
    script: "scripts/check-model-benchmark-evidence.ts",
    args: ["--require-representative-platforms"],
  });
}

if (requireBranchProtection) {
  checks.push({
    label: "branch protection native backend contexts",
    script: "scripts/check-branch-protection.ts",
    args: ["--repo", branchProtectionRepo, "--branch", branchProtectionBranch],
  });
}

for (const check of checks) {
  if (!runCheck(check)) {
    failures.push(`${check.label} failed.`);
  }
}

if (failures.length > 0) {
  console.error("Release readiness evidence check failed:");
  for (const failure of failures) {
    console.error(`- ${failure}`);
  }
  process.exit(1);
}

console.log(
  `Release readiness evidence check passed for ${checks.length} gate(s).`,
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

function readNativeSmokePlatform(dir: string): string | null {
  const summaryPath = path.join(dir, "native-smoke-summary.json");
  try {
    const summary = JSON.parse(readFileSync(summaryPath, "utf8")) as {
      platform?: unknown;
    };
    if (
      summary.platform === "win32" ||
      summary.platform === "darwin" ||
      summary.platform === "linux"
    ) {
      return summary.platform;
    }
    failures.push(
      `${summaryPath} platform must be win32, darwin, or linux, got ${String(summary.platform)}.`,
    );
  } catch (error) {
    failures.push(`${summaryPath} could not be read: ${error}`);
  }
  return null;
}

function runCheck(check: Check): boolean {
  console.log(`\n==> ${check.label}`);
  const result = spawnSync(
    process.execPath,
    [path.normalize(check.script), ...check.args],
    {
      stdio: "inherit",
      shell: process.platform === "win32",
    },
  );
  if (result.error) {
    console.error(`${check.label} failed to start: ${result.error.message}`);
    return false;
  }
  return result.status === 0;
}
