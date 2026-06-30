import { describe, expect, test } from "bun:test";
import type {
  AndroidAsrDownloadProgress,
  AndroidAsrModelPackState,
} from "./bridge";
import { clearModelProgress, clearProgressEntry } from "./modelProgress";

const pack = (
  overrides: Partial<AndroidAsrModelPackState> = {},
): AndroidAsrModelPackState => ({
  id: "asr-pack",
  displayName: "ASR pack",
  description: "Test ASR pack",
  language: "en",
  sizeMb: 1,
  minRamMb: 0,
  installedDir: "",
  isInstalled: false,
  isDownloading: true,
  isActive: false,
  isSelectable: false,
  downloadPhase: "installing",
  downloadProgress: 100,
  missingFiles: ["tokens.txt"],
  ...overrides,
});

const progress = (
  overrides: Partial<AndroidAsrDownloadProgress> = {},
): AndroidAsrDownloadProgress => ({
  modelId: "asr-pack",
  phase: "installing",
  file: null,
  downloaded: 0,
  total: 0,
  percentage: 100,
  ...overrides,
});

describe("Android model progress helpers", () => {
  test("clears stale failed-download progress from the affected card", () => {
    const otherPack = pack({ id: "other-pack", downloadPhase: "downloading" });
    const next = clearModelProgress([pack(), otherPack], "asr-pack");

    expect(next[0]).toMatchObject({
      isDownloading: false,
      downloadPhase: "available",
      downloadProgress: 0,
    });
    expect(next[1]).toBe(otherPack);
  });

  test("returns an installed affected card to ready progress", () => {
    const next = clearModelProgress(
      [
        pack({
          isInstalled: true,
          isSelectable: true,
          missingFiles: [],
        }),
      ],
      "asr-pack",
    );

    expect(next[0]).toMatchObject({
      isDownloading: false,
      downloadPhase: "ready",
      downloadProgress: 100,
    });
  });

  test("removes only the stale progress entry for the failed model", () => {
    const otherProgress = progress({
      modelId: "other-pack",
      phase: "downloading",
      percentage: 20,
    });

    expect(
      clearProgressEntry(
        {
          "asr-pack": progress(),
          "other-pack": otherProgress,
        },
        "asr-pack",
      ),
    ).toEqual({
      "other-pack": otherProgress,
    });
  });
});
