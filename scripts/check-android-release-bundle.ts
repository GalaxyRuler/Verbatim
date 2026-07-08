import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { inflateRawSync } from "node:zlib";

const DEV_URL_MARKER = "localhost:1420";
const RUST_LIBRARY_NAME = "libverbatim_app_lib.so";
// tauri-codegen embeds frontendDist into the library; the Vite entry document
// starts with a doctype, so its presence proves the frontend was compiled in.
const FRONTEND_MARKER = "<!doctype html";
const FRONTEND_MARKER_UPPER = "<!DOCTYPE html";

const args = process.argv.slice(2);

if (args.includes("--help") || args.includes("-h") || args.length === 0) {
  console.log(`Usage: bun scripts/check-android-release-bundle.ts <apk|aab> [...]

Verifies Android release artifacts load the bundled frontend instead of the
dev server URL. Release builds embed the frontend into the Rust library via
tauri/custom-protocol, so for every supplied APK/AAB this fails when:
- ${RUST_LIBRARY_NAME} is missing (release Rust build did not run), or
- any ${RUST_LIBRARY_NAME} contains "${DEV_URL_MARKER}" (devUrl baked in;
  tauri/custom-protocol was not enabled for the release Rust build), or
- any ${RUST_LIBRARY_NAME} lacks an embedded HTML document (the bundled
  frontend was not compiled into the library).

Note: the APK/AAB also carries assets/tauri.conf.json, but Android only reads
its "plugins" section (PluginManager.loadConfig), so a devUrl in that asset is
inert and is not checked here.`);
  process.exit(args.length === 0 ? 1 : 0);
}

const failures: string[] = [];

for (const input of args) {
  checkArtifact(input);
}

if (failures.length > 0) {
  console.error("Android release bundle check failed:");
  for (const failure of failures) {
    console.error(`- ${failure}`);
  }
  process.exit(1);
}

console.log(
  `Android release bundle check passed for ${args.length} artifact(s).`,
);

function checkArtifact(input: string): void {
  const resolved = path.resolve(input);

  if (!existsSync(resolved)) {
    fail(`Input path does not exist: ${input}`);
  }

  if (!/\.(apk|aab|zip)$/i.test(resolved)) {
    fail(`Unsupported input path, expected .apk or .aab: ${input}`);
  }

  const label = path.basename(resolved);
  const entries = readZipEntries(readFileSync(resolved), label);

  const rustLibraries = entries.filter(
    (entry) => path.posix.basename(entry.name) === RUST_LIBRARY_NAME,
  );
  if (rustLibraries.length === 0) {
    failures.push(`${label}: no ${RUST_LIBRARY_NAME} found in the artifact`);
  }
  for (const library of rustLibraries) {
    const data = library.data();
    if (data.includes(DEV_URL_MARKER)) {
      failures.push(
        `${label}: ${library.name} contains "${DEV_URL_MARKER}"; the release Rust build was compiled without tauri/custom-protocol or with build.devUrl set`,
      );
    }
    if (
      !data.includes(FRONTEND_MARKER) &&
      !data.includes(FRONTEND_MARKER_UPPER)
    ) {
      failures.push(
        `${label}: ${library.name} has no embedded HTML document; the bundled frontend was not compiled into the release library`,
      );
    }
  }
}

interface ZipEntry {
  name: string;
  data: () => Buffer;
}

// Minimal read-only ZIP parser (central directory + stored/deflated entries).
// External extractors are unreliable across platforms: unzip prompts on
// duplicate names, GNU tar mistakes `C:` for a remote host, jar needs a JDK.
function readZipEntries(archive: Buffer, label: string): ZipEntry[] {
  const EOCD_SIGNATURE = 0x06054b50;
  const CENTRAL_SIGNATURE = 0x02014b50;

  const searchStart = Math.max(0, archive.length - 64 * 1024 - 22);
  let eocdOffset = -1;
  for (let i = archive.length - 22; i >= searchStart; i--) {
    if (archive.readUInt32LE(i) === EOCD_SIGNATURE) {
      eocdOffset = i;
      break;
    }
  }
  if (eocdOffset === -1) {
    fail(`${label}: not a ZIP archive (end of central directory not found)`);
  }

  const entryCount = archive.readUInt16LE(eocdOffset + 10);
  let offset = archive.readUInt32LE(eocdOffset + 16);
  const entries: ZipEntry[] = [];

  for (let i = 0; i < entryCount; i++) {
    if (archive.readUInt32LE(offset) !== CENTRAL_SIGNATURE) {
      fail(`${label}: corrupt ZIP central directory at offset ${offset}`);
    }

    const method = archive.readUInt16LE(offset + 10);
    const compressedSize = archive.readUInt32LE(offset + 20);
    const nameLength = archive.readUInt16LE(offset + 28);
    const extraLength = archive.readUInt16LE(offset + 30);
    const commentLength = archive.readUInt16LE(offset + 32);
    const localOffset = archive.readUInt32LE(offset + 42);
    const name = archive
      .subarray(offset + 46, offset + 46 + nameLength)
      .toString("utf8")
      .replaceAll("\\", "/");

    entries.push({
      name,
      data: () => {
        // The local header repeats name/extra with possibly different lengths.
        const localNameLength = archive.readUInt16LE(localOffset + 26);
        const localExtraLength = archive.readUInt16LE(localOffset + 28);
        const dataStart = localOffset + 30 + localNameLength + localExtraLength;
        const raw = archive.subarray(dataStart, dataStart + compressedSize);
        if (method === 0) {
          return raw;
        }
        if (method === 8) {
          return inflateRawSync(raw);
        }
        fail(
          `${label}: unsupported ZIP compression method ${method} for ${name}`,
        );
      },
    });

    offset += 46 + nameLength + extraLength + commentLength;
  }

  return entries;
}

function fail(message: string): never {
  console.error(message);
  process.exit(1);
}
