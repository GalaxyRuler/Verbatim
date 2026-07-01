import { existsSync, lstatSync, readdirSync } from "node:fs";
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

export function sleep(ms: number): Promise<void> {
  return new Promise((resolveWait) => setTimeout(resolveWait, ms));
}

export async function waitForPathMissing(
  filePath: string,
  maxMs: number,
  intervalMs = 250,
): Promise<boolean> {
  const started = Date.now();
  while (Date.now() - started < maxMs) {
    if (!existsSync(filePath)) return true;
    await sleep(intervalMs);
  }
  return !existsSync(filePath);
}

export async function waitForPathsMissing(
  filePaths: string[],
  maxMs: number,
  intervalMs = 250,
): Promise<string[]> {
  const uniquePaths = [...new Set(filePaths)];
  const started = Date.now();
  while (Date.now() - started < maxMs) {
    const remainingPaths = uniquePaths.filter((filePath) =>
      existsSync(filePath),
    );
    if (remainingPaths.length === 0) return [];
    await sleep(intervalMs);
  }
  return uniquePaths.filter((filePath) => existsSync(filePath));
}

export async function retryOnce<T>(
  operation: (attempt: number) => Promise<T>,
  options: {
    delayMs?: number;
    onFailure?: (error: unknown, attempt: number) => void | Promise<void>;
  } = {},
): Promise<T> {
  let lastError: unknown;
  for (const attempt of [1, 2]) {
    try {
      return await operation(attempt);
    } catch (error) {
      lastError = error;
      await options.onFailure?.(error, attempt);
      if (attempt === 1) {
        await sleep(options.delayMs ?? 0);
      }
    }
  }
  throw lastError;
}
