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
import { findFiles, findMountedMacApp } from "./installer-smoke-utils.js";

function withTempDir<T>(callback: (dir: string) => T): T {
  const dir = mkdtempSync(join(tmpdir(), "installer-smoke-utils-"));
  try {
    return callback(dir);
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
});
