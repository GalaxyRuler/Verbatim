import { spawnSync } from "node:child_process";
import {
  existsSync,
  mkdtempSync,
  readdirSync,
  rmSync,
  statSync,
} from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";

const MIN_LOAD_ALIGN = 16 * 1024;
const args = process.argv.slice(2);

if (args.includes("--help") || args.includes("-h") || args.length === 0) {
  console.log(`Usage: bun scripts/check-android-so-alignment.ts <apk|jniLibs|so> [...]

Checks every Android shared object under the supplied APK, jniLibs directory,
or .so path. Fails when any LOAD segment alignment is below 16 KB.`);
  process.exit(args.length === 0 ? 1 : 0);
}

const objdump = findLlvmObjdump();
const tempDirs: string[] = [];
const failures: string[] = [];

try {
  const sharedObjects = args.flatMap((input) => collectSharedObjects(input));

  if (sharedObjects.length === 0) {
    fail("No Android .so files found in the supplied input paths.");
  }

  for (const soPath of sharedObjects) {
    const result = spawnSync(objdump, ["-p", soPath], {
      encoding: "utf8",
      maxBuffer: 64 * 1024 * 1024,
    });

    if (result.error) {
      failures.push(
        `${soPath}: failed to start llvm-objdump: ${result.error.message}`,
      );
      continue;
    }

    if (result.status !== 0) {
      failures.push(`${soPath}: llvm-objdump failed: ${result.stderr.trim()}`);
      continue;
    }

    const loadAlignments = parseLoadAlignments(result.stdout);
    if (loadAlignments.length === 0) {
      failures.push(`${soPath}: no LOAD segments found`);
      continue;
    }

    for (const alignment of loadAlignments) {
      if (alignment < MIN_LOAD_ALIGN) {
        failures.push(
          `${soPath}: LOAD segment align ${alignment} is below ${MIN_LOAD_ALIGN}`,
        );
      }
    }
  }

  if (failures.length > 0) {
    console.error("Android .so 16 KB alignment check failed:");
    for (const failure of failures) {
      console.error(`- ${failure}`);
    }
    process.exit(1);
  }

  console.log(
    `Android .so 16 KB alignment check passed for ${sharedObjects.length} file(s).`,
  );
} finally {
  for (const tempDir of tempDirs) {
    rmSync(tempDir, { force: true, recursive: true });
  }
}

function collectSharedObjects(input: string): string[] {
  const resolved = path.resolve(input);

  if (!existsSync(resolved)) {
    fail(`Input path does not exist: ${input}`);
  }

  const stats = statSync(resolved);
  if (stats.isDirectory()) {
    return findSharedObjects(resolved);
  }

  if (resolved.endsWith(".so")) {
    return [resolved];
  }

  if (resolved.endsWith(".apk") || resolved.endsWith(".zip")) {
    const extractDir = mkdtempSync(path.join(tmpdir(), "verbatim-android-so-"));
    tempDirs.push(extractDir);
    extractZip(resolved, extractDir);
    return findSharedObjects(path.join(extractDir, "lib"));
  }

  fail(
    `Unsupported input path, expected .apk, .zip, .so, or directory: ${input}`,
  );
}

function findSharedObjects(root: string): string[] {
  if (!existsSync(root)) {
    return [];
  }

  const entries = readdirSync(root, { withFileTypes: true });
  const results: string[] = [];

  for (const entry of entries) {
    const entryPath = path.join(root, entry.name);
    if (entry.isDirectory()) {
      results.push(...findSharedObjects(entryPath));
    } else if (entry.isFile() && entry.name.endsWith(".so")) {
      results.push(entryPath);
    }
  }

  return results;
}

function extractZip(archive: string, destination: string): void {
  const unzip = spawnSync("unzip", ["-q", archive, "-d", destination], {
    encoding: "utf8",
  });
  if (unzip.status === 0) {
    return;
  }

  const jar = spawnSync("jar", ["xf", archive], {
    cwd: destination,
    encoding: "utf8",
  });
  if (jar.status !== 0) {
    fail(
      `Failed to extract ${archive}. unzip: ${unzip.stderr.trim()} jar: ${jar.stderr.trim()}`,
    );
  }
}

function parseLoadAlignments(output: string): number[] {
  return output
    .split(/\r?\n/)
    .filter((line) => /^\s*LOAD\s/.test(line))
    .map((line) => {
      const match = line.match(/\balign\s+(\S+)/);
      if (!match) {
        fail(`Unable to parse LOAD alignment from llvm-objdump line: ${line}`);
      }
      return parseAlignment(match[1]);
    });
}

function parseAlignment(value: string): number {
  const powerMatch = value.match(/^2\*\*(\d+)$/);
  if (powerMatch) {
    return 2 ** Number(powerMatch[1]);
  }

  if (value.startsWith("0x")) {
    return Number.parseInt(value, 16);
  }

  const decimal = Number.parseInt(value, 10);
  if (Number.isFinite(decimal)) {
    return decimal;
  }

  fail(`Unsupported LOAD alignment value: ${value}`);
}

function findLlvmObjdump(): string {
  const explicit = process.env.LLVM_OBJDUMP;
  if (explicit && existsSync(explicit)) {
    return explicit;
  }

  for (const envName of ["ANDROID_NDK_HOME", "ANDROID_NDK_ROOT", "NDK_HOME"]) {
    const ndkDir = process.env[envName];
    if (!ndkDir) {
      continue;
    }

    const prebuiltDir = path.join(ndkDir, "toolchains", "llvm", "prebuilt");
    if (!existsSync(prebuiltDir)) {
      continue;
    }

    for (const host of readdirSync(prebuiltDir)) {
      const candidate = path.join(
        prebuiltDir,
        host,
        "bin",
        process.platform === "win32" ? "llvm-objdump.exe" : "llvm-objdump",
      );
      if (existsSync(candidate)) {
        return candidate;
      }
    }
  }

  return "llvm-objdump";
}

function fail(message: string): never {
  console.error(message);
  process.exit(1);
}
