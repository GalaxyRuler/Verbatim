import { mkdir, stat, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { parseArgs } from "node:util";

const { values } = parseArgs({
  options: {
    version: { type: "string" },
    output: { type: "string" },
  },
});

const version = values.version;
const output = values.output;

if (!version) {
  throw new Error("Missing required --version.");
}

if (!output) {
  throw new Error("Missing required --output.");
}

const semverWithNumericBuild =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)-[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*\+[0-9]+$/;

if (!semverWithNumericBuild.test(version)) {
  throw new Error(
    `DevVersion '${version}' must be valid SemVer with prerelease and numeric build metadata, for example 0.8.9-dev.202606171606+24144.`,
  );
}

const config = {
  productName: "Verbatim Dev",
  mainBinaryName: "verbatim-dev",
  version,
  identifier: "com.galaxyruler.verbatim.dev",
  plugins: {
    updater: {
      endpoints: ["https://127.0.0.1:9/verbatim-dev/latest.json"],
    },
  },
};

const outputPath = resolve(output);
const outputDirectory = dirname(outputPath);
try {
  const outputDirectoryStat = await stat(outputDirectory);
  if (!outputDirectoryStat.isDirectory()) {
    throw new Error(
      `Output parent path is not a directory: ${outputDirectory}`,
    );
  }
} catch (error) {
  if ((error as NodeJS.ErrnoException).code !== "ENOENT") {
    throw error;
  }
  await mkdir(outputDirectory, { recursive: true });
}
await writeFile(outputPath, `${JSON.stringify(config, null, 2)}\n`, "utf8");

console.log(`DevConfig=${outputPath}`);
console.log(`DevVersion=${version}`);
