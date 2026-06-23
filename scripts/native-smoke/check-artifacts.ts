import { existsSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";

type NativeSmokeSummary = {
  platform?: string;
  files?: string[];
  phases?: Array<{ name?: string; code?: number | null }>;
  controlledDesktopTargets?: ControlledDesktopTargetEvidence;
  appInsertionDrills?: AppInsertionEvidence;
};

type NativeSmokeStatus = {
  startup_status?: string | { status?: string };
  settings_loaded?: boolean;
  audio_fixture_verified?: boolean;
  resource_probe_checked?: boolean;
  retention?: {
    clean_profile_verified?: boolean;
    failures?: string[];
  } | null;
};

type ControlledDesktopTargetEvidence = {
  checked?: boolean;
  target_launched?: boolean;
  target_focused?: boolean;
  text_entry_checked?: boolean;
  text_entry_verified?: boolean | null;
  clipboard_mutation_checked?: boolean;
  clipboard_mutation_preserved_marker?: boolean | null;
  failures?: string[];
};

type VirtualAudioInputEvidence = {
  checked?: boolean;
  smoke_microphone_arg?: string | null;
  failures?: string[];
};

type VirtualAudioPlaybackEvidence = {
  checked?: boolean;
  playback_attempted?: boolean;
  playback_device?: string | null;
  failures?: string[];
};

type VirtualAudioCleanupEvidence = {
  checked?: boolean;
  cleanup_attempted?: boolean;
  cleanup_commands?: string[][];
  unloaded_module_ids?: string[];
  failures?: string[];
};

type AppInsertionEvidence = {
  checked?: boolean;
  required?: boolean;
  cases?: Array<{
    case?: string;
    app_driven?: boolean;
    passed?: boolean;
    desktop_target?: string | null;
    inference_started?: boolean;
    focus_switched_before_insertion?: boolean;
    insertion_blocked?: boolean;
    paste_attempted?: boolean;
    clipboard_mutated_after_verbatim_write?: boolean;
    user_clipboard_preserved?: boolean;
    user_clipboard_contents_recorded?: boolean;
    failures?: string[];
  }>;
};

const args = process.argv.slice(2);

if (args.includes("--help") || args.includes("-h")) {
  console.log(`Usage: bun scripts/native-smoke/check-artifacts.ts [options]

Options:
  --dir <path>                    Native smoke artifact directory.
  --require-installer             Require installer-smoke artifacts.
  --require-desktop-target        Require controlled desktop target evidence.
  --require-virtual-audio         Require virtual-audio input/playback/cleanup evidence.
  --require-app-insertion-drills  Require full app-driven insertion race evidence.
  --require-platform <platform>   Require native-smoke-summary.json platform to match.
  --help                          Show this help text.
`);
  process.exit(0);
}

const artifactDir = resolve(argValue("--dir") ?? "native-smoke-artifacts");
const requireInstaller = hasArg("--require-installer");
const requireDesktopTarget = hasArg("--require-desktop-target");
const requireVirtualAudio = hasArg("--require-virtual-audio");
const requireAppInsertionDrills = hasArg("--require-app-insertion-drills");
const requiredPlatform = argValue("--require-platform");
const failures: string[] = [];

function argValue(name: string): string | undefined {
  const index = args.indexOf(name);
  if (index >= 0) return args[index + 1];
  const prefix = `${name}=`;
  return args.find((arg) => arg.startsWith(prefix))?.slice(prefix.length);
}

function hasArg(name: string): boolean {
  return args.includes(name);
}

const summary = readJson<NativeSmokeSummary>("native-smoke-summary.json");
const firstLaunch = readJson<NativeSmokeStatus>("first-launch.status.json");

if (summary) validateSummary(summary, "native-smoke-summary.json");
if (firstLaunch) validateFirstLaunchStatus(firstLaunch, "first-launch");
requireFile("first-launch.stdout.log");
requireFile("first-launch.stderr.log");
requireScreenshotEvidence("before");
requireScreenshotEvidence("after");

if (requireInstaller) validateInstallerArtifacts();
if (requireDesktopTarget) validateControlledDesktopTarget(summary);
if (requireVirtualAudio) validateVirtualAudioArtifacts();
if (requireAppInsertionDrills) validateAppInsertionEvidence(summary);

if (failures.length > 0) {
  console.error("Native smoke artifact check failed:");
  for (const failure of failures) {
    console.error(`- ${failure}`);
  }
  process.exit(1);
}

console.log(`Native smoke artifact check passed for ${artifactDir}.`);

function readJson<T>(relativePath: string): T | null {
  const fullPath = join(artifactDir, relativePath);
  if (!existsSync(fullPath)) {
    failures.push(`${relativePath} is missing.`);
    return null;
  }

  try {
    return JSON.parse(readFileSync(fullPath, "utf8")) as T;
  } catch (error) {
    failures.push(`${relativePath} is not valid JSON: ${error}`);
    return null;
  }
}

function readOptionalJson<T>(relativePath: string): T | null {
  const fullPath = join(artifactDir, relativePath);
  if (!existsSync(fullPath)) return null;
  try {
    return JSON.parse(readFileSync(fullPath, "utf8")) as T;
  } catch (error) {
    failures.push(`${relativePath} is not valid JSON: ${error}`);
    return null;
  }
}

function requireFile(relativePath: string): void {
  if (!existsSync(join(artifactDir, relativePath))) {
    failures.push(`${relativePath} is missing.`);
  }
}

function requireScreenshotEvidence(label: string): void {
  if (
    !existsSync(join(artifactDir, `${label}.png`)) &&
    !existsSync(join(artifactDir, `${label}-screenshot-skipped.txt`))
  ) {
    failures.push(`${label} screenshot or screenshot-skip note is missing.`);
  }
}

function validateSummary(summary: NativeSmokeSummary, label: string): void {
  if (requiredPlatform && summary.platform !== requiredPlatform) {
    failures.push(
      `${label} platform must be ${requiredPlatform}, got ${String(summary.platform)}.`,
    );
  }
  if (!Array.isArray(summary.files) || summary.files.length === 0) {
    failures.push(`${label} files must be a non-empty array.`);
  }
  if (!Array.isArray(summary.phases) || summary.phases.length === 0) {
    failures.push(`${label} phases must be a non-empty array.`);
    return;
  }

  for (const phase of summary.phases) {
    if (phase.code !== 0) {
      failures.push(
        `phase ${String(phase.name)} exited with code ${String(phase.code)}.`,
      );
    }
  }
}

function validateFirstLaunchStatus(
  status: NativeSmokeStatus,
  label: string,
): void {
  const startupStatus =
    typeof status.startup_status === "string"
      ? status.startup_status
      : status.startup_status?.status;
  if (startupStatus !== "ready") {
    failures.push(
      `${label} startup_status must be ready, got ${String(startupStatus)}.`,
    );
  }
  if (status.settings_loaded !== true) {
    failures.push(`${label} settings_loaded must be true.`);
  }
  if (status.audio_fixture_verified !== true) {
    failures.push(`${label} audio_fixture_verified must be true.`);
  }
  if (status.resource_probe_checked !== true) {
    failures.push(`${label} resource_probe_checked must be true.`);
  }
  if (status.retention?.clean_profile_verified !== true) {
    failures.push(`${label} retention.clean_profile_verified must be true.`);
  }
  for (const failure of status.retention?.failures ?? []) {
    failures.push(`${label} retention failure: ${failure}`);
  }
}

function validateInstallerArtifacts(): void {
  requireFile("installer/installer-smoke-summary.json");
  requireFile("installer/packaged-smoke/native-smoke-summary.json");
  requireFile("installer/packaged-smoke/first-launch.status.json");

  const installerSummary = readOptionalJson<{ installedAppPath?: string }>(
    "installer/installer-smoke-summary.json",
  );
  if (
    installerSummary &&
    (typeof installerSummary.installedAppPath !== "string" ||
      installerSummary.installedAppPath.trim() === "")
  ) {
    failures.push(
      "installer-smoke-summary.json installedAppPath must be present.",
    );
  }

  const nestedSummary = readOptionalJson<NativeSmokeSummary>(
    "installer/packaged-smoke/native-smoke-summary.json",
  );
  if (nestedSummary) {
    validateSummary(
      nestedSummary,
      "installer/packaged-smoke/native-smoke-summary.json",
    );
  }

  const nestedFirstLaunch = readOptionalJson<NativeSmokeStatus>(
    "installer/packaged-smoke/first-launch.status.json",
  );
  if (nestedFirstLaunch) {
    validateFirstLaunchStatus(
      nestedFirstLaunch,
      "installer/packaged-smoke/first-launch",
    );
  }
}

function validateControlledDesktopTarget(
  summary: NativeSmokeSummary | null,
): void {
  const evidence =
    summary?.controlledDesktopTargets ??
    readOptionalJson<ControlledDesktopTargetEvidence>(
      "controlled-desktop-targets.json",
    );

  if (!evidence) {
    failures.push("controlled-desktop-targets.json evidence is missing.");
    return;
  }
  if (evidence.checked !== true) {
    failures.push("controlled desktop target evidence must be checked.");
  }
  if (evidence.target_launched !== true) {
    failures.push("controlled desktop target must be launched.");
  }
  if (evidence.target_focused !== true) {
    failures.push("controlled desktop target must be focused.");
  }
  if (evidence.text_entry_checked && evidence.text_entry_verified !== true) {
    failures.push("controlled desktop text-entry marker was not verified.");
  }
  if (
    evidence.clipboard_mutation_checked &&
    evidence.clipboard_mutation_preserved_marker !== true
  ) {
    failures.push(
      "controlled desktop clipboard mutation marker was not verified.",
    );
  }
  for (const failure of evidence.failures ?? []) {
    failures.push(`controlled desktop target failure: ${failure}`);
  }
}

function validateVirtualAudioArtifacts(): void {
  const input = readJson<VirtualAudioInputEvidence>("virtual-audio-input.json");
  const playback = readJson<VirtualAudioPlaybackEvidence>(
    "virtual-audio-playback.json",
  );
  const cleanup = readJson<VirtualAudioCleanupEvidence>(
    "virtual-audio-cleanup.json",
  );

  if (input) {
    if (input.checked !== true)
      failures.push("virtual-audio-input checked must be true.");
    if (!input.smoke_microphone_arg) {
      failures.push("virtual-audio-input smoke_microphone_arg is missing.");
    }
    for (const failure of input.failures ?? []) {
      failures.push(`virtual-audio-input failure: ${failure}`);
    }
  }

  if (playback) {
    if (playback.checked !== true)
      failures.push("virtual-audio-playback checked must be true.");
    if (playback.playback_attempted !== true) {
      failures.push("virtual-audio-playback playback_attempted must be true.");
    }
    if (!playback.playback_device) {
      failures.push("virtual-audio-playback playback_device is missing.");
    }
    for (const failure of playback.failures ?? []) {
      failures.push(`virtual-audio-playback failure: ${failure}`);
    }
  }

  if (cleanup) {
    if (cleanup.checked !== true)
      failures.push("virtual-audio-cleanup checked must be true.");
    const cleanupCommands = cleanup.cleanup_commands ?? [];
    if (cleanupCommands.length > 0) {
      if (cleanup.cleanup_attempted !== true) {
        failures.push("virtual-audio-cleanup cleanup_attempted must be true.");
      }
      if (
        (cleanup.unloaded_module_ids ?? []).length !== cleanupCommands.length
      ) {
        failures.push(
          "virtual-audio-cleanup did not unload every reported module.",
        );
      }
    }
    for (const failure of cleanup.failures ?? []) {
      failures.push(`virtual-audio-cleanup failure: ${failure}`);
    }
  }
}

function validateAppInsertionEvidence(
  summary: NativeSmokeSummary | null,
): void {
  const evidence =
    summary?.appInsertionDrills ??
    readOptionalJson<AppInsertionEvidence>("app-insertion-drills.json");
  if (!evidence) {
    failures.push("app insertion drill evidence is missing.");
    return;
  }

  if (evidence.checked !== true) {
    failures.push("app insertion drill evidence must be checked.");
  }
  const cases = evidence.cases ?? [];
  const focusCase = cases.find(
    (item) => item.case === "focus_switch_during_inference_blocks_insertion",
  );
  const clipboardCase = cases.find(
    (item) =>
      item.case === "clipboard_mutation_during_paste_preserves_user_clipboard",
  );

  if (!focusCase) {
    failures.push("focus-switch app insertion case is missing.");
  } else {
    requireAppDrivenCase(focusCase, "focus-switch");
    if (focusCase.inference_started !== true) {
      failures.push("focus-switch case must start inference.");
    }
    if (focusCase.focus_switched_before_insertion !== true) {
      failures.push("focus-switch case must switch focus before insertion.");
    }
    if (focusCase.insertion_blocked !== true) {
      failures.push("focus-switch case must block insertion.");
    }
  }

  if (!clipboardCase) {
    failures.push("clipboard-mutation app insertion case is missing.");
  } else {
    requireAppDrivenCase(clipboardCase, "clipboard-mutation");
    if (clipboardCase.paste_attempted !== true) {
      failures.push("clipboard-mutation case must attempt paste.");
    }
    if (clipboardCase.clipboard_mutated_after_verbatim_write !== true) {
      failures.push(
        "clipboard-mutation case must mutate clipboard after Verbatim write.",
      );
    }
    if (clipboardCase.user_clipboard_preserved !== true) {
      failures.push("clipboard-mutation case must preserve user clipboard.");
    }
    if (clipboardCase.user_clipboard_contents_recorded !== false) {
      failures.push(
        "clipboard-mutation case must not record user clipboard contents.",
      );
    }
  }
}

function requireAppDrivenCase(
  item: NonNullable<AppInsertionEvidence["cases"]>[number],
  label: string,
): void {
  if (item.app_driven !== true)
    failures.push(`${label} case must be app-driven.`);
  if (item.passed !== true) failures.push(`${label} case must pass.`);
  if (
    typeof item.desktop_target !== "string" ||
    item.desktop_target.length === 0
  ) {
    failures.push(`${label} case must name the desktop target.`);
  }
  for (const failure of item.failures ?? []) {
    failures.push(`${label} case failure: ${failure}`);
  }
}
