import type {
  AndroidAsrDownloadProgress,
  AndroidAsrModelPackState,
  AndroidLlmDownloadProgress,
  AndroidLlmModelPackState,
} from "./bridge";

export type AndroidModelPackState =
  | AndroidAsrModelPackState
  | AndroidLlmModelPackState;
export type AndroidModelDownloadProgress =
  | AndroidAsrDownloadProgress
  | AndroidLlmDownloadProgress;

const ACTIVE_DOWNLOAD_PHASES = new Set([
  "downloading",
  "verifying",
  "installing",
]);

export function applyModelProgress<T extends AndroidModelPackState>(
  packs: T[],
  progress: AndroidModelDownloadProgress,
): T[] {
  return packs.map((pack) =>
    pack.id === progress.modelId
      ? ({
          ...pack,
          isDownloading: ACTIVE_DOWNLOAD_PHASES.has(progress.phase),
          downloadPhase: progress.phase,
          downloadProgress: progress.percentage,
        } as T)
      : pack,
  );
}

export function clearModelProgress<T extends AndroidModelPackState>(
  packs: T[],
  modelId: string,
): T[] {
  return packs.map((pack) =>
    pack.id === modelId
      ? ({
          ...pack,
          isDownloading: false,
          downloadPhase: pack.isInstalled ? "ready" : "available",
          downloadProgress: pack.isInstalled ? 100 : 0,
        } as T)
      : pack,
  );
}

export function clearProgressEntry(
  progressById: Record<string, AndroidModelDownloadProgress>,
  modelId: string,
): Record<string, AndroidModelDownloadProgress> {
  const next = { ...progressById };
  delete next[modelId];
  return next;
}
