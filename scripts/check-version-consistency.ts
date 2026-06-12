import { readFile } from "node:fs/promises";
import path from "node:path";

const STABLE_SEMVER = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;

type VersionSource = {
  name: string;
  version: string;
};

function fail(message: string): never {
  console.error(message);
  process.exit(1);
}

function readTomlPackageVersion(contents: string): string {
  const packageSection = contents.match(/\[package\]([\s\S]*?)(?:\n\[|$)/);
  const version = packageSection?.[1].match(/^\s*version\s*=\s*"([^"]+)"/m);

  if (!version) {
    fail("Could not find [package] version in src-tauri/Cargo.toml.");
  }

  return version[1];
}

function readCargoLockPackageVersion(contents: string): string {
  const packageSection = contents.match(
    /\[\[package\]\]\s+name = "verbatim"\s+version = "([^"]+)"/,
  );

  if (!packageSection) {
    fail("Could not find verbatim package version in src-tauri/Cargo.lock.");
  }

  return packageSection[1];
}

function validateStableSemver(version: string): void {
  if (!STABLE_SEMVER.test(version)) {
    fail(
      [
        `Version '${version}' is not a stable SemVer core version.`,
        "Use MAJOR.MINOR.PATCH, for example 0.8.7 or 1.0.0.",
        "Do not use fixed-width padding such as 0.08.007; leading zeroes are not SemVer-compatible.",
      ].join("\n"),
    );
  }
}

function requestedTag(): string | undefined {
  const tagArg = process.argv.find((arg) => arg.startsWith("--tag="));
  if (tagArg) {
    return tagArg.slice("--tag=".length);
  }

  if (process.env.RELEASE_TAG) {
    return process.env.RELEASE_TAG;
  }

  if (process.env.GITHUB_REF_TYPE === "tag") {
    return process.env.GITHUB_REF_NAME;
  }

  return undefined;
}

const repoRoot = process.cwd();
const packageJsonPath = path.join(repoRoot, "package.json");
const tauriConfigPath = path.join(repoRoot, "src-tauri", "tauri.conf.json");
const cargoTomlPath = path.join(repoRoot, "src-tauri", "Cargo.toml");
const cargoLockPath = path.join(repoRoot, "src-tauri", "Cargo.lock");

const packageJson = JSON.parse(await readFile(packageJsonPath, "utf8")) as {
  version?: string;
};
const tauriConfig = JSON.parse(await readFile(tauriConfigPath, "utf8")) as {
  version?: string;
};
const cargoToml = await readFile(cargoTomlPath, "utf8");
const cargoLock = await readFile(cargoLockPath, "utf8");

const sources: VersionSource[] = [
  {
    name: "package.json",
    version: packageJson.version ?? "",
  },
  {
    name: "src-tauri/tauri.conf.json",
    version: tauriConfig.version ?? "",
  },
  {
    name: "src-tauri/Cargo.toml",
    version: readTomlPackageVersion(cargoToml),
  },
  {
    name: "src-tauri/Cargo.lock",
    version: readCargoLockPackageVersion(cargoLock),
  },
];

for (const source of sources) {
  if (!source.version) {
    fail(`Missing version in ${source.name}.`);
  }
  validateStableSemver(source.version);
}

const canonicalVersion = sources[0].version;
const mismatches = sources.filter(
  (source) => source.version !== canonicalVersion,
);

if (mismatches.length > 0) {
  const details = sources
    .map((source) => `- ${source.name}: ${source.version}`)
    .join("\n");
  fail(`Version mismatch detected.\n${details}`);
}

const tag = requestedTag();
if (tag && tag !== `v${canonicalVersion}`) {
  fail(
    `Release tag '${tag}' does not match expected tag 'v${canonicalVersion}'.`,
  );
}

console.log(`Version OK: ${canonicalVersion}`);
