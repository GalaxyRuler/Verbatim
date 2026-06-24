import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";

type PackageJson = {
  name?: string;
  version?: string;
  dependencies?: Record<string, string>;
  devDependencies?: Record<string, string>;
};

type CargoPackage = {
  name: string;
  version: string;
  source?: string;
  checksum?: string;
};

const outputDir = process.argv.includes("--out")
  ? process.argv[process.argv.indexOf("--out") + 1]
  : "dist/release-metadata";
const checkOnly = process.argv.includes("--check");

if (!outputDir) {
  throw new Error("--out requires a directory");
}

const root = process.cwd();
const packageJson = JSON.parse(
  readFileSync(path.join(root, "package.json"), "utf8"),
) as PackageJson;
const cargoLock = readFileSync(
  path.join(root, "src-tauri", "Cargo.lock"),
  "utf8",
);

const generatedAt =
  process.env.SOURCE_DATE_EPOCH !== undefined
    ? new Date(Number(process.env.SOURCE_DATE_EPOCH) * 1000).toISOString()
    : new Date().toISOString();
const revision =
  process.env.GITHUB_SHA ?? process.env.VERBATIM_RELEASE_REVISION ?? "local";
const version =
  process.env.VERBATIM_RELEASE_VERSION ?? packageJson.version ?? "0.0.0";

function parseCargoPackages(lockfile: string): CargoPackage[] {
  return lockfile
    .split(/\n\[\[package\]\]\n/g)
    .slice(1)
    .map((block) => {
      const get = (field: string) =>
        block.match(new RegExp(`^${field} = "([^"]+)"`, "m"))?.[1];
      return {
        name: get("name") ?? "",
        version: get("version") ?? "",
        source: get("source"),
        checksum: get("checksum"),
      };
    })
    .filter((pkg) => pkg.name && pkg.version)
    .sort((a, b) =>
      `${a.name}@${a.version}`.localeCompare(`${b.name}@${b.version}`),
    );
}

function frontendPackages(
  packageMap: Record<string, string> | undefined,
  scope: string,
) {
  return Object.entries(packageMap ?? {})
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([name, version]) => ({
      SPDXID: `SPDXRef-npm-${name.replace(/[^A-Za-z0-9.-]/g, "-")}-${scope}`,
      name,
      versionInfo: version,
      supplier: "NOASSERTION",
      downloadLocation: "NOASSERTION",
      filesAnalyzed: false,
      licenseConcluded: "NOASSERTION",
      licenseDeclared: "NOASSERTION",
      copyrightText: "NOASSERTION",
      externalRefs: [
        {
          referenceCategory: "PACKAGE-MANAGER",
          referenceType: "purl",
          referenceLocator: `pkg:npm/${encodeURIComponent(name)}@${encodeURIComponent(version)}`,
        },
      ],
    }));
}

const cargoPackages = parseCargoPackages(cargoLock).map((pkg) => ({
  SPDXID: `SPDXRef-cargo-${pkg.name.replace(/[^A-Za-z0-9.-]/g, "-")}-${pkg.version.replace(/[^A-Za-z0-9.-]/g, "-")}`,
  name: pkg.name,
  versionInfo: pkg.version,
  supplier: "NOASSERTION",
  downloadLocation: pkg.source ?? "NOASSERTION",
  filesAnalyzed: false,
  licenseConcluded: "NOASSERTION",
  licenseDeclared: "NOASSERTION",
  copyrightText: "NOASSERTION",
  checksums: pkg.checksum
    ? [{ algorithm: "SHA256", checksumValue: pkg.checksum }]
    : undefined,
  externalRefs: [
    {
      referenceCategory: "PACKAGE-MANAGER",
      referenceType: "purl",
      referenceLocator: `pkg:cargo/${encodeURIComponent(pkg.name)}@${encodeURIComponent(pkg.version)}`,
    },
  ],
}));

const rootPackage = {
  SPDXID: "SPDXRef-Verbatim",
  name: packageJson.name ?? "verbatim-app",
  versionInfo: version,
  supplier: "Organization: GalaxyRuler",
  downloadLocation: "NOASSERTION",
  filesAnalyzed: false,
  licenseConcluded: "MIT",
  licenseDeclared: "MIT",
  copyrightText: "NOASSERTION",
};

const packages = [
  rootPackage,
  ...frontendPackages(packageJson.dependencies, "runtime"),
  ...frontendPackages(packageJson.devDependencies, "dev"),
  ...cargoPackages,
];

const sbom = {
  spdxVersion: "SPDX-2.3",
  dataLicense: "CC0-1.0",
  SPDXID: "SPDXRef-DOCUMENT",
  name: `Verbatim ${version} SBOM`,
  documentNamespace: `https://github.com/GalaxyRuler/Verbatim/releases/download/v${version}/sbom-${revision}`,
  creationInfo: {
    created: generatedAt,
    creators: ["Tool: scripts/generate-release-metadata.ts"],
  },
  packages,
  relationships: packages
    .filter((pkg) => pkg.SPDXID !== rootPackage.SPDXID)
    .map((pkg) => ({
      spdxElementId: rootPackage.SPDXID,
      relationshipType: "DEPENDS_ON",
      relatedSpdxElement: pkg.SPDXID,
    })),
};

const provenance = {
  predicateType: "https://slsa.dev/provenance/v1",
  subject: {
    name: packageJson.name ?? "verbatim-app",
    version,
  },
  buildDefinition: {
    buildType: "https://github.com/actions/workflow",
    externalParameters: {
      repository: process.env.GITHUB_REPOSITORY ?? "local",
      workflow: process.env.GITHUB_WORKFLOW ?? "local",
      ref: process.env.GITHUB_REF ?? "local",
      run_id: process.env.GITHUB_RUN_ID ?? "local",
      run_attempt: process.env.GITHUB_RUN_ATTEMPT ?? "local",
    },
  },
  runDetails: {
    builder: {
      id: process.env.RUNNER_NAME ?? "local",
    },
    metadata: {
      invocationId: process.env.GITHUB_RUN_ID ?? "local",
      startedOn: generatedAt,
    },
  },
  materials: [
    {
      uri: `git+https://github.com/${process.env.GITHUB_REPOSITORY ?? "GalaxyRuler/Verbatim"}`,
      digest: {
        sha1: revision,
      },
    },
  ],
};

mkdirSync(outputDir, { recursive: true });
const sbomPath = path.join(outputDir, "SBOM.spdx.json");
const provenancePath = path.join(outputDir, "RELEASE_PROVENANCE.json");

const sbomText = `${JSON.stringify(sbom, null, 2)}\n`;
const provenanceText = `${JSON.stringify(provenance, null, 2)}\n`;

if (checkOnly) {
  JSON.parse(sbomText);
  JSON.parse(provenanceText);
  if (packages.length < 2) {
    throw new Error("Generated SBOM does not include dependency packages");
  }
  console.log(
    `Release metadata check passed: ${packages.length} SBOM packages, provenance revision ${revision}.`,
  );
} else {
  writeFileSync(sbomPath, sbomText);
  writeFileSync(provenancePath, provenanceText);
  console.log(`Wrote ${sbomPath}`);
  console.log(`Wrote ${provenancePath}`);
}
