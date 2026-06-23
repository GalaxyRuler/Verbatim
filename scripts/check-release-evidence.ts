import { createHash } from "node:crypto";
import { existsSync, readFileSync, statSync } from "node:fs";
import path from "node:path";

type ReleaseManifestAsset = {
  name?: string;
  size_bytes?: number;
  sha256?: string;
  content_type?: string;
  browser_download_url?: string;
  updater_platform_key?: string | null;
  updater_signature_in_latest_json?: boolean;
  updater_signature_asset_present?: boolean;
  signing_enabled?: boolean;
  signing_identity?: string | null;
  provenance_url?: string | null;
  sbom_url?: string | null;
};

type ReleaseManifest = {
  version?: string;
  generated_at?: string;
  signing_enabled?: boolean;
  assets?: ReleaseManifestAsset[];
};

type LatestJson = {
  platforms?: Record<string, { url?: string; signature?: string }>;
};

type AttestationEvidence = {
  asset?: string;
  verified?: boolean;
  repository?: string;
  command?: string;
  verified_at?: string;
  subject_sha256?: string;
};

const args = process.argv.slice(2);

if (args.includes("--help") || args.includes("-h")) {
  console.log(`Usage: bun scripts/check-release-evidence.ts [options]

Options:
  --dir <path>              Directory containing RELEASE_MANIFEST.json, SHA256SUMS.txt, latest.json, and assets.
  --signed                  Require signed-release manifest fields.
  --require-attestations    Require retained gh attestation verification evidence for packaged desktop assets.
  --attestations-dir <path> Directory containing <asset>.attestation.json files. Defaults to <dir>/attestations.
  --help                    Show this help text.
`);
  process.exit(0);
}

const evidenceDir = path.resolve(argValue("--dir") ?? "dist/release-evidence");
const requireSigned = args.includes("--signed");
const requireAttestations = args.includes("--require-attestations");
const attestationsDir = path.resolve(
  argValue("--attestations-dir") ?? path.join(evidenceDir, "attestations"),
);
const requiredDesktopAssets = [
  /Verbatim_.+_x64-setup\.exe$/,
  /Verbatim_.+_x64_en-US\.msi$/,
  /Verbatim_.+_aarch64\.dmg$/,
  /Verbatim_.+_amd64\.deb$/,
];
const requiredUpdaterPlatforms = [
  "darwin-aarch64",
  "linux-x86_64",
  "windows-x86_64",
];
const failures: string[] = [];

function argValue(name: string): string | undefined {
  const index = args.indexOf(name);
  if (index >= 0) return args[index + 1];
  const prefix = `${name}=`;
  return args.find((arg) => arg.startsWith(prefix))?.slice(prefix.length);
}

function readJson<T>(fileName: string): T | null {
  const filePath = path.join(evidenceDir, fileName);
  if (!existsSync(filePath)) {
    failures.push(`${fileName} is missing from ${evidenceDir}`);
    return null;
  }
  try {
    return JSON.parse(readFileSync(filePath, "utf8")) as T;
  } catch (error) {
    failures.push(`${fileName} is not valid JSON: ${error}`);
    return null;
  }
}

function readSha256Sums(): Map<string, string> {
  const filePath = path.join(evidenceDir, "SHA256SUMS.txt");
  const checksums = new Map<string, string>();
  if (!existsSync(filePath)) {
    failures.push(`SHA256SUMS.txt is missing from ${evidenceDir}`);
    return checksums;
  }

  const lines = readFileSync(filePath, "utf8")
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);

  for (const line of lines) {
    const match = line.match(/^([a-f0-9]{64})\s{2}(.+)$/i);
    if (!match) {
      failures.push(`SHA256SUMS.txt has invalid line: ${line}`);
      continue;
    }
    checksums.set(match[2], match[1].toLowerCase());
  }

  return checksums;
}

const manifest = readJson<ReleaseManifest>("RELEASE_MANIFEST.json");
const latestJson = readJson<LatestJson>("latest.json");
const checksums = readSha256Sums();

if (manifest) validateManifest(manifest, checksums);
if (latestJson && manifest) validateLatestJson(latestJson, manifest);

if (failures.length > 0) {
  console.error("Release evidence check failed:");
  for (const failure of failures) {
    console.error(`- ${failure}`);
  }
  process.exit(1);
}

console.log(
  `Release evidence check passed for ${manifest?.assets?.length ?? 0} manifest asset(s).`,
);

function validateManifest(
  manifest: ReleaseManifest,
  checksums: Map<string, string>,
): void {
  if (typeof manifest.version !== "string" || manifest.version.trim() === "") {
    failures.push("RELEASE_MANIFEST.json version must be a non-empty string.");
  }
  if (
    typeof manifest.generated_at !== "string" ||
    Number.isNaN(Date.parse(manifest.generated_at))
  ) {
    failures.push("RELEASE_MANIFEST.json generated_at must be an ISO date.");
  }
  if (!Array.isArray(manifest.assets) || manifest.assets.length === 0) {
    failures.push("RELEASE_MANIFEST.json assets must be a non-empty array.");
    return;
  }

  if (requireSigned && manifest.signing_enabled !== true) {
    failures.push("Signed release evidence requires signing_enabled=true.");
  }

  const assetNames = manifest.assets
    .map((asset) => asset.name)
    .filter((name): name is string => typeof name === "string");
  for (const pattern of requiredDesktopAssets) {
    if (!assetNames.some((name) => pattern.test(name))) {
      failures.push(`Missing desktop asset matching ${pattern.source}.`);
    }
  }

  for (const asset of manifest.assets) {
    validateAsset(asset, checksums);
  }

  if (requireAttestations) {
    validateAttestationEvidence(manifest.assets);
  }
}

function validateAsset(
  asset: ReleaseManifestAsset,
  checksums: Map<string, string>,
): void {
  const name = asset.name;
  if (typeof name !== "string" || name.trim() === "") {
    failures.push("Manifest asset is missing a non-empty name.");
    return;
  }
  if (typeof asset.size_bytes !== "number" || asset.size_bytes <= 0) {
    failures.push(`${name} size_bytes must be positive.`);
  }
  if (!/^[a-f0-9]{64}$/i.test(asset.sha256 ?? "")) {
    failures.push(`${name} sha256 must be a 64-character hex digest.`);
  }
  if (checksums.get(name) !== asset.sha256?.toLowerCase()) {
    failures.push(`${name} SHA256SUMS.txt digest does not match manifest.`);
  }
  if (typeof asset.browser_download_url !== "string") {
    failures.push(`${name} browser_download_url is missing.`);
  }
  if (!asset.provenance_url) {
    failures.push(`${name} provenance_url is missing.`);
  }
  if (!asset.sbom_url) {
    failures.push(`${name} sbom_url is missing.`);
  }
  if (requireSigned) {
    if (asset.signing_enabled !== true) {
      failures.push(`${name} signing_enabled must be true for signed release.`);
    }
    if (
      typeof asset.signing_identity !== "string" ||
      asset.signing_identity.trim() === ""
    ) {
      failures.push(
        `${name} signing_identity must name the public signing identity.`,
      );
    }
  }

  const assetPath = path.join(evidenceDir, name);
  if (!existsSync(assetPath)) return;

  const stat = statSync(assetPath);
  if (stat.size !== asset.size_bytes) {
    failures.push(
      `${name} file size ${stat.size} does not match manifest ${asset.size_bytes}.`,
    );
  }

  const digest = createHash("sha256")
    .update(readFileSync(assetPath))
    .digest("hex");
  if (digest !== asset.sha256?.toLowerCase()) {
    failures.push(`${name} file digest does not match manifest.`);
  }
}

function validateAttestationEvidence(assets: ReleaseManifestAsset[]): void {
  for (const pattern of requiredDesktopAssets) {
    const asset = assets.find(
      (candidate) =>
        typeof candidate.name === "string" && pattern.test(candidate.name),
    );
    if (!asset?.name) continue;

    const evidencePath = path.join(
      attestationsDir,
      `${asset.name}.attestation.json`,
    );
    if (!existsSync(evidencePath)) {
      failures.push(
        `${asset.name} attestation evidence is missing at ${evidencePath}.`,
      );
      continue;
    }

    let evidence: AttestationEvidence;
    try {
      evidence = JSON.parse(
        readFileSync(evidencePath, "utf8"),
      ) as AttestationEvidence;
    } catch (error) {
      failures.push(`${evidencePath} is not valid JSON: ${error}`);
      continue;
    }

    if (evidence.asset !== asset.name) {
      failures.push(`${evidencePath} asset must be ${asset.name}.`);
    }
    if (evidence.verified !== true) {
      failures.push(`${evidencePath} verified must be true.`);
    }
    if (
      typeof evidence.repository !== "string" ||
      evidence.repository.trim() === ""
    ) {
      failures.push(`${evidencePath} repository must be a non-empty string.`);
    }
    if (
      typeof evidence.command !== "string" ||
      !/\bgh\s+attestation\s+verify\b/.test(evidence.command)
    ) {
      failures.push(
        `${evidencePath} command must record gh attestation verify.`,
      );
    }
    if (
      typeof evidence.verified_at !== "string" ||
      Number.isNaN(Date.parse(evidence.verified_at))
    ) {
      failures.push(`${evidencePath} verified_at must be an ISO date.`);
    }
    if (evidence.subject_sha256 !== asset.sha256?.toLowerCase()) {
      failures.push(
        `${evidencePath} subject_sha256 must match RELEASE_MANIFEST.json.`,
      );
    }
  }
}

function validateLatestJson(
  latestJson: LatestJson,
  manifest: ReleaseManifest,
): void {
  const assetNames = new Set(manifest.assets?.map((asset) => asset.name));
  for (const platform of requiredUpdaterPlatforms) {
    const entry = latestJson.platforms?.[platform];
    if (!entry?.url || !entry.signature) {
      failures.push(`latest.json is missing url/signature for ${platform}.`);
      continue;
    }

    const assetName = decodeURIComponent(
      new URL(entry.url).pathname.split("/").pop() ?? "",
    );
    if (!assetNames.has(assetName)) {
      failures.push(
        `latest.json ${platform} references missing asset ${assetName}.`,
      );
    }
    if (!assetNames.has(`${assetName}.sig`)) {
      failures.push(
        `latest.json ${platform} signature asset ${assetName}.sig is missing.`,
      );
    }

    const manifestAsset = manifest.assets?.find(
      (asset) => asset.name === assetName,
    );
    if (manifestAsset?.updater_platform_key !== platform) {
      failures.push(`${assetName} updater_platform_key must be ${platform}.`);
    }
    if (manifestAsset?.updater_signature_in_latest_json !== true) {
      failures.push(
        `${assetName} updater_signature_in_latest_json must be true.`,
      );
    }
    if (manifestAsset?.updater_signature_asset_present !== true) {
      failures.push(
        `${assetName} updater_signature_asset_present must be true.`,
      );
    }
  }
}
