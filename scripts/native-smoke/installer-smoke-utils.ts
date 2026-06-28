import { lstatSync, readdirSync } from "node:fs";
import { basename, join } from "node:path";

export function findMountedMacApp(
  mountDir: string,
  expectedAppName = "Verbatim.app",
): string {
  const topLevelApps = readdirSync(mountDir)
    .sort((left, right) => left.localeCompare(right))
    .flatMap((entry) => {
      const fullPath = join(mountDir, entry);
      const stat = lstatSync(fullPath);
      if (
        stat.isSymbolicLink() ||
        !stat.isDirectory() ||
        !entry.endsWith(".app")
      ) {
        return [];
      }
      return [fullPath];
    });

  const expectedApp = topLevelApps.find(
    (candidate) => basename(candidate) === expectedAppName,
  );
  const mountedApp = expectedApp ?? topLevelApps[0];
  if (!mountedApp) {
    throw new Error(
      `No non-symlink top-level .app bundle found under ${mountDir}`,
    );
  }
  return mountedApp;
}

export function findFiles(root: string, extension: string): string[] {
  const files: string[] = [];
  const walk = (current: string) => {
    for (const entry of readdirSync(current)) {
      const fullPath = join(current, entry);
      const stat = lstatSync(fullPath);
      if (stat.isSymbolicLink()) {
        continue;
      }
      if (stat.isDirectory()) {
        if (fullPath.endsWith(extension)) {
          files.push(fullPath);
        } else {
          walk(fullPath);
        }
      } else if (entry.endsWith(extension)) {
        files.push(fullPath);
      }
    }
  };
  walk(root);
  return files;
}
