import { describe, expect, test } from "bun:test";
import {
  mkdirSync,
  mkdtempSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  findFiles,
  findMountedMacApp,
  retryOnce,
  waitForPathMissing,
  waitForPathsMissing,
} from "./installer-smoke-utils.js";

function withTempDir<T>(callback: (dir: string) => T): T {
  const dir = mkdtempSync(join(tmpdir(), "installer-smoke-utils-"));
  try {
    return callback(dir);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

async function withTempDirAsync<T>(
  callback: (dir: string) => Promise<T>,
): Promise<T> {
  const dir = mkdtempSync(join(tmpdir(), "installer-smoke-utils-"));
  try {
    return await callback(dir);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

describe("installer smoke utilities", () => {
  test("selects the top-level Verbatim app instead of recursing into Applications", () => {
    withTempDir((mountDir) => {
      mkdirSync(join(mountDir, "Other.app"), { recursive: true });
      mkdirSync(join(mountDir, "Verbatim.app"), { recursive: true });
      mkdirSync(join(mountDir, "Applications"), { recursive: true });
      mkdirSync(join(mountDir, "Applications", "IDLE.app"), {
        recursive: true,
      });

      expect(findMountedMacApp(mountDir)).toBe(join(mountDir, "Verbatim.app"));
    });
  });

  test("falls back to the first top-level app when Verbatim.app is absent", () => {
    withTempDir((mountDir) => {
      mkdirSync(join(mountDir, "Alpha.app"), { recursive: true });
      mkdirSync(join(mountDir, "Beta.app"), { recursive: true });

      expect(findMountedMacApp(mountDir)).toBe(join(mountDir, "Alpha.app"));
    });
  });

  test("does not traverse symlinked directories while finding files", () => {
    withTempDir((tempRoot) => {
      const root = join(tempRoot, "scan-root");
      const realDir = join(tempRoot, "real");
      const linkedDir = join(root, "linked");
      mkdirSync(root, { recursive: true });
      mkdirSync(realDir, { recursive: true });
      writeFileSync(join(realDir, "hidden.dmg"), "not from mounted root");

      try {
        symlinkSync(realDir, linkedDir, "dir");
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code === "EPERM") {
          return;
        }
        throw error;
      }

      expect(findFiles(root, ".dmg")).toEqual([]);
    });
  });

  test("waits for a path to disappear during a bounded polling window", async () => {
    await withTempDirAsync(async (tempRoot) => {
      const marker = join(tempRoot, "marker.txt");
      writeFileSync(marker, "pending deletion");

      setTimeout(() => rmSync(marker, { force: true }), 20);

      expect(await waitForPathMissing(marker, 500, 10)).toBe(true);
    });
  });

  test("reports paths that remain after the polling timeout", async () => {
    await withTempDirAsync(async (tempRoot) => {
      const removedMarker = join(tempRoot, "removed.txt");
      const remainingMarker = join(tempRoot, "remaining.txt");
      writeFileSync(removedMarker, "pending deletion");
      writeFileSync(remainingMarker, "still present");

      rmSync(removedMarker, { force: true });

      expect(
        await waitForPathsMissing([removedMarker, remainingMarker], 50, 10),
      ).toEqual([remainingMarker]);
    });
  });

  test("retries an operation once after a transient failure", async () => {
    let attempts = 0;
    const failures: Array<{ attempt: number; message: string }> = [];

    const result = await retryOnce(
      async (attempt) => {
        attempts += 1;
        if (attempt === 1) {
          throw new Error("transient uninstall race");
        }
        return "recovered";
      },
      {
        delayMs: 1,
        onFailure: (error, attempt) => {
          failures.push({
            attempt,
            message: error instanceof Error ? error.message : String(error),
          });
        },
      },
    );

    expect(result).toBe("recovered");
    expect(attempts).toBe(2);
    expect(failures).toEqual([
      { attempt: 1, message: "transient uninstall race" },
    ]);
  });
});
