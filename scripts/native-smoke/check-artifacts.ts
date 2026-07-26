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
  real_inference?: {
    checked?: boolean;
    model_id?: string;
    audio_sample_count?: number;
    model_loaded?: boolean;
    inference_started?: boolean;
    inference_completed?: boolean;
    transcript_non_empty?: boolean;
    transcript_recorded?: boolean;
    failure_class?: string | null;
  } | null;
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

type GoldenDictationInsertionReceipt = {
  attempted?: boolean;
  succeeded?: boolean;
  target_verified?: boolean;
  error?: string | null;
};

type GoldenDictationCase = {
  name?: string;
  app_driven?: boolean;
  fixture_rendered_to_virtual_output?: boolean;
  model_inference_completed?: boolean;
  model_inference_reached_insertion?: boolean;
  model_inference_reached_clipboard_write?: boolean;
  insertion?: GoldenDictationInsertionReceipt;
  controlled_target_has_nonempty_insertion?: boolean;
  origin_target_unchanged?: boolean;
  replacement_target_unchanged?: boolean;
  synthetic_clipboard_mutation_preserved?: boolean;
  clipboard_precondition_empty?: boolean;
  controlled_target_unchanged?: boolean;
};

type GoldenDictationStartupCase = {
  process_started?: boolean;
  settings_file_observed?: boolean;
  app_exited_before_settings?: boolean;
  app_exit_code?: number | null;
};

type GoldenDictationStartupEvidence = {
  stable_focus?: GoldenDictationStartupCase;
  focus_switch?: GoldenDictationStartupCase;
  clipboard_mutation?: GoldenDictationStartupCase;
};

type GoldenDictationTargetEvidence = {
  process_started?: boolean;
  main_window_observed?: boolean;
  process_exited_before_window?: boolean;
  focus_activation_requested?: boolean;
  focus_confirmed?: boolean;
};

type GoldenDictationTargetsEvidence = {
  stable_focus?: GoldenDictationTargetEvidence;
  focus_origin?: GoldenDictationTargetEvidence;
  focus_replacement?: GoldenDictationTargetEvidence;
  clipboard_mutation?: GoldenDictationTargetEvidence;
};

type GoldenDictationCaptureEvidence = {
  target_focused_before_start?: boolean;
  recording_start_requested?: boolean;
  target_focused_before_playback?: boolean;
  playback_invoked?: boolean;
  playback_completed?: boolean;
  recording_stop_requested?: boolean;
};

type GoldenDictationCapturesEvidence = {
  stable_focus?: GoldenDictationCaptureEvidence;
  focus_switch?: GoldenDictationCaptureEvidence;
  clipboard_mutation?: GoldenDictationCaptureEvidence;
};

type GoldenDictationEvidence = {
  schema_version?: number;
  runner?: string;
  isolated_profile?: boolean;
  transcript_recorded?: boolean;
  audio_recorded?: boolean;
  input_device_name?: string;
  output_device_name?: string;
  model_id?: string;
  startup?: GoldenDictationStartupEvidence;
  targets?: GoldenDictationTargetsEvidence;
  capture?: GoldenDictationCapturesEvidence;
  cases?: GoldenDictationCase[];
  failure_class?: string | null;
  failure_stage?: string | null;
  failure_detail?: string | null;
};

const args = process.argv.slice(2);

if (args.includes("--help") || args.includes("-h")) {
  console.log(`Usage: bun scripts/native-smoke/check-artifacts.ts [options]

Options:
  --dir <path>                    Native smoke artifact directory.
  --require-installer             Require installer-smoke artifacts.
  --require-desktop-target        Require controlled desktop target evidence.
  --require-virtual-audio         Require virtual-audio input/playback/cleanup evidence.
  --require-real-inference        Require redacted local-model inference evidence.
  --require-app-insertion-drills  Require full app-driven insertion race evidence.
  --require-golden-dictation      Require the physical virtual-audio dictation contract.
  --golden-dictation-only         Validate only golden-dictation evidence (requires the flag above).
  --require-platform <platform>   Require native-smoke-summary.json platform to match.
  --help                          Show this help text.
`);
  process.exit(0);
}

const artifactDir = resolve(argValue("--dir") ?? "native-smoke-artifacts");
const requireInstaller = hasArg("--require-installer");
const requireDesktopTarget = hasArg("--require-desktop-target");
const requireVirtualAudio = hasArg("--require-virtual-audio");
const requireRealInference = hasArg("--require-real-inference");
const requireAppInsertionDrills = hasArg("--require-app-insertion-drills");
const requireGoldenDictation = hasArg("--require-golden-dictation");
const goldenDictationOnly = hasArg("--golden-dictation-only");
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

if (goldenDictationOnly && !requireGoldenDictation) {
  failures.push("--golden-dictation-only requires --require-golden-dictation.");
}

let summary: NativeSmokeSummary | null = null;
if (!goldenDictationOnly) {
  summary = readJson<NativeSmokeSummary>("native-smoke-summary.json");
  const firstLaunch = readJson<NativeSmokeStatus>("first-launch.status.json");

  if (summary) validateSummary(summary, "native-smoke-summary.json");
  if (firstLaunch) validateFirstLaunchStatus(firstLaunch, "first-launch");
  requireFile("first-launch.stdout.log");
  requireFile("first-launch.stderr.log");
  requireScreenshotEvidence("before");
  requireScreenshotEvidence("after");
}

if (requireInstaller) validateInstallerArtifacts();
if (requireDesktopTarget) validateControlledDesktopTarget(summary);
if (requireVirtualAudio) validateVirtualAudioArtifacts();
if (requireRealInference) validateRealInferenceArtifacts();
if (requireAppInsertionDrills) validateAppInsertionEvidence(summary);
if (requireGoldenDictation) validateGoldenDictationEvidence();

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

function validateRealInferenceArtifacts(): void {
  const status = readJson<NativeSmokeStatus>("real-inference.status.json");
  const inference = status?.real_inference;
  if (!inference) {
    failures.push(
      "real-inference.status.json real_inference evidence is missing.",
    );
    return;
  }

  if (inference.checked !== true) {
    failures.push("real inference evidence must be checked.");
  }
  if (
    typeof inference.model_id !== "string" ||
    inference.model_id.trim() === ""
  ) {
    failures.push("real inference model_id is missing.");
  }
  if ((inference.audio_sample_count ?? 0) <= 0) {
    failures.push("real inference audio_sample_count must be positive.");
  }
  if (inference.model_loaded !== true) {
    failures.push("real inference model_loaded must be true.");
  }
  if (inference.inference_started !== true) {
    failures.push("real inference inference_started must be true.");
  }
  if (inference.inference_completed !== true) {
    failures.push("real inference inference_completed must be true.");
  }
  if (inference.transcript_non_empty !== true) {
    failures.push("real inference transcript_non_empty must be true.");
  }
  if (inference.transcript_recorded !== false) {
    failures.push(
      "real inference must not retain transcript content in artifacts.",
    );
  }
  if (inference.failure_class) {
    failures.push(`real inference failure_class=${inference.failure_class}.`);
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

function validateGoldenDictationEvidence(): void {
  const evidence = readJson<GoldenDictationEvidence>("golden-dictation.json");
  if (!evidence) return;

  requireExactKeys(
    evidence,
    [
      "schema_version",
      "runner",
      "isolated_profile",
      "transcript_recorded",
      "audio_recorded",
      "input_device_name",
      "output_device_name",
      "model_id",
      "startup",
      "targets",
      "capture",
      "cases",
      "failure_class",
      "failure_stage",
      "failure_detail",
    ],
    "golden-dictation.json",
  );
  if (evidence.schema_version !== 1) {
    failures.push("golden-dictation schema_version must be 1.");
  }
  if (evidence.runner !== "windows_interactive_golden_dictation") {
    failures.push(
      "golden-dictation runner must be windows_interactive_golden_dictation.",
    );
  }
  if (evidence.isolated_profile !== true) {
    failures.push("golden-dictation must use an isolated profile.");
  }
  if (evidence.transcript_recorded !== false) {
    failures.push("golden-dictation must not retain transcript content.");
  }
  if (evidence.audio_recorded !== false) {
    failures.push("golden-dictation must not retain dictated audio.");
  }
  if (!nonEmptyString(evidence.input_device_name)) {
    failures.push("golden-dictation input_device_name is missing.");
  }
  if (!nonEmptyString(evidence.output_device_name)) {
    failures.push("golden-dictation output_device_name is missing.");
  }
  if (!nonEmptyString(evidence.model_id)) {
    failures.push("golden-dictation model_id is missing.");
  }
  validateGoldenStartupEvidence(evidence.startup);
  validateGoldenTargetsEvidence(evidence.targets);
  validateGoldenCaptureEvidence(evidence.capture);
  if (evidence.failure_class !== null) {
    failures.push(
      `golden-dictation failure_class must be null, got ${String(evidence.failure_class)}.`,
    );
  }
  if (evidence.failure_stage !== null) {
    failures.push(
      `golden-dictation failure_stage must be null, got ${String(evidence.failure_stage)}.`,
    );
  }
  if (evidence.failure_detail !== null) {
    failures.push(
      `golden-dictation failure_detail must be null, got ${String(evidence.failure_detail)}.`,
    );
  }

  const cases = evidence.cases;
  if (!Array.isArray(cases) || cases.length !== 3) {
    failures.push("golden-dictation must contain exactly three case results.");
    return;
  }

  const stable = cases.find((item) => item.name === "stable_focus_inserts");
  const focus = cases.find(
    (item) => item.name === "focus_switch_during_inference_blocks_insertion",
  );
  const clipboard = cases.find(
    (item) =>
      item.name === "clipboard_mutation_during_paste_preserves_user_clipboard",
  );

  if (!stable) {
    failures.push("golden-dictation stable-focus case is missing.");
  } else {
    validateGoldenStableCase(stable);
  }
  if (!focus) {
    failures.push("golden-dictation focus-switch case is missing.");
  } else {
    validateGoldenFocusCase(focus);
  }
  if (!clipboard) {
    failures.push("golden-dictation clipboard-mutation case is missing.");
  } else {
    validateGoldenClipboardCase(clipboard);
  }

  if (
    nonEmptyString(evidence.output_device_name) &&
    nonEmptyString(evidence.input_device_name)
  ) {
    validateGoldenPlaybackArtifacts(evidence.output_device_name);
  }
  validateGoldenReceiptArtifact(
    "stable-focus.insertion.jsonl",
    "stable_focus_inserts",
    { attempted: true, succeeded: true, targetVerified: true, error: null },
  );
  validateGoldenReceiptArtifact(
    "focus-switch.insertion.jsonl",
    "focus_switch_during_inference_blocks_insertion",
    {
      attempted: false,
      succeeded: false,
      targetVerified: false,
      error: "target changed before insertion",
    },
  );
  validateGoldenReceiptArtifact(
    "clipboard-mutation.insertion.jsonl",
    "clipboard_mutation_during_paste_preserves_user_clipboard",
    {
      attempted: true,
      succeeded: false,
      targetVerified: true,
      error: "clipboard changed before paste",
    },
  );
}

function validateGoldenCaptureEvidence(
  capture: GoldenDictationCapturesEvidence | undefined,
): void {
  if (!capture) {
    failures.push("golden-dictation capture evidence is missing.");
    return;
  }
  requireExactKeys(
    capture,
    ["stable_focus", "focus_switch", "clipboard_mutation"],
    "golden-dictation capture evidence",
  );
  const requiredKeys = [
    "target_focused_before_start",
    "recording_start_requested",
    "target_focused_before_playback",
    "playback_invoked",
    "playback_completed",
    "recording_stop_requested",
  ];
  for (const [label, item] of [
    ["stable-focus", capture.stable_focus],
    ["focus-switch", capture.focus_switch],
    ["clipboard-mutation", capture.clipboard_mutation],
  ] as const) {
    if (!item) {
      failures.push(`golden-dictation ${label} capture evidence is missing.`);
      continue;
    }
    requireExactKeys(
      item,
      requiredKeys,
      `golden-dictation ${label} capture evidence`,
    );
    for (const key of requiredKeys) {
      if (item[key as keyof GoldenDictationCaptureEvidence] !== true) {
        failures.push(
          `golden-dictation ${label} capture step ${key} was not completed.`,
        );
      }
    }
  }
}

function validateGoldenTargetsEvidence(
  targets: GoldenDictationTargetsEvidence | undefined,
): void {
  if (!targets) {
    failures.push("golden-dictation target evidence is missing.");
    return;
  }
  requireExactKeys(
    targets,
    ["stable_focus", "focus_origin", "focus_replacement", "clipboard_mutation"],
    "golden-dictation target evidence",
  );
  for (const [label, item] of [
    ["stable-focus", targets.stable_focus],
    ["focus-origin", targets.focus_origin],
    ["focus-replacement", targets.focus_replacement],
    ["clipboard-mutation", targets.clipboard_mutation],
  ] as const) {
    if (!item) {
      failures.push(`golden-dictation ${label} target evidence is missing.`);
      continue;
    }
    requireExactKeys(
      item,
      [
        "process_started",
        "main_window_observed",
        "process_exited_before_window",
        "focus_activation_requested",
        "focus_confirmed",
      ],
      `golden-dictation ${label} target evidence`,
    );
    if (item.process_started !== true) {
      failures.push(`golden-dictation ${label} target process did not start.`);
    }
    if (item.main_window_observed !== true) {
      failures.push(
        `golden-dictation ${label} target window was not observed.`,
      );
    }
    if (item.process_exited_before_window !== false) {
      failures.push(
        `golden-dictation ${label} target exited before its window was ready.`,
      );
    }
    if (
      item.focus_activation_requested !== true ||
      item.focus_confirmed !== true
    ) {
      failures.push(
        `golden-dictation ${label} target focus was not confirmed.`,
      );
    }
  }
}

function validateGoldenStartupEvidence(
  startup: GoldenDictationStartupEvidence | undefined,
): void {
  if (!startup) {
    failures.push("golden-dictation startup evidence is missing.");
    return;
  }
  requireExactKeys(
    startup,
    ["stable_focus", "focus_switch", "clipboard_mutation"],
    "golden-dictation startup evidence",
  );
  for (const [label, item] of [
    ["stable-focus", startup.stable_focus],
    ["focus-switch", startup.focus_switch],
    ["clipboard-mutation", startup.clipboard_mutation],
  ] as const) {
    if (!item) {
      failures.push(`golden-dictation ${label} startup evidence is missing.`);
      continue;
    }
    requireExactKeys(
      item,
      [
        "process_started",
        "settings_file_observed",
        "app_exited_before_settings",
        "app_exit_code",
      ],
      `golden-dictation ${label} startup evidence`,
    );
    if (item.process_started !== true) {
      failures.push(`golden-dictation ${label} app process did not start.`);
    }
    if (item.settings_file_observed !== true) {
      failures.push(
        `golden-dictation ${label} settings file was not observed.`,
      );
    }
    if (item.app_exited_before_settings !== false) {
      failures.push(
        `golden-dictation ${label} app exited before settings were ready.`,
      );
    }
    if (item.app_exit_code !== null) {
      failures.push(`golden-dictation ${label} app_exit_code must be null.`);
    }
  }
}

function validateGoldenStableCase(item: GoldenDictationCase): void {
  requireExactKeys(
    item,
    [
      "name",
      "app_driven",
      "fixture_rendered_to_virtual_output",
      "model_inference_completed",
      "insertion",
      "controlled_target_has_nonempty_insertion",
    ],
    "golden-dictation stable-focus case",
  );
  if (item.app_driven !== true) {
    failures.push("golden-dictation stable-focus case must be app-driven.");
  }
  if (item.fixture_rendered_to_virtual_output !== true) {
    failures.push("golden-dictation stable-focus fixture was not rendered.");
  }
  if (item.model_inference_completed !== true) {
    failures.push("golden-dictation stable-focus inference did not complete.");
  }
  if (item.controlled_target_has_nonempty_insertion !== true) {
    failures.push(
      "golden-dictation stable-focus did not insert nonempty text.",
    );
  }
  validateGoldenInsertion(
    item.insertion,
    "golden-dictation stable-focus insertion",
    { attempted: true, succeeded: true, targetVerified: true, error: null },
  );
}

function validateGoldenFocusCase(item: GoldenDictationCase): void {
  requireExactKeys(
    item,
    [
      "name",
      "app_driven",
      "fixture_rendered_to_virtual_output",
      "model_inference_reached_insertion",
      "insertion",
      "origin_target_unchanged",
      "replacement_target_unchanged",
    ],
    "golden-dictation focus-switch case",
  );
  if (item.app_driven !== true) {
    failures.push("golden-dictation focus-switch case must be app-driven.");
  }
  if (item.fixture_rendered_to_virtual_output !== true) {
    failures.push("golden-dictation focus-switch fixture was not rendered.");
  }
  if (item.model_inference_reached_insertion !== true) {
    failures.push(
      "golden-dictation focus-switch did not reach the insertion barrier.",
    );
  }
  if (item.origin_target_unchanged !== true) {
    failures.push("golden-dictation focus-switch mutated the origin target.");
  }
  if (item.replacement_target_unchanged !== true) {
    failures.push(
      "golden-dictation focus-switch mutated the replacement target.",
    );
  }
  validateGoldenInsertion(
    item.insertion,
    "golden-dictation focus-switch insertion",
    {
      attempted: false,
      succeeded: false,
      targetVerified: false,
      error: "target changed before insertion",
    },
  );
}

function validateGoldenClipboardCase(item: GoldenDictationCase): void {
  requireExactKeys(
    item,
    [
      "name",
      "app_driven",
      "fixture_rendered_to_virtual_output",
      "model_inference_reached_clipboard_write",
      "insertion",
      "synthetic_clipboard_mutation_preserved",
      "clipboard_precondition_empty",
      "controlled_target_unchanged",
    ],
    "golden-dictation clipboard-mutation case",
  );
  if (item.app_driven !== true) {
    failures.push(
      "golden-dictation clipboard-mutation case must be app-driven.",
    );
  }
  if (item.fixture_rendered_to_virtual_output !== true) {
    failures.push(
      "golden-dictation clipboard-mutation fixture was not rendered.",
    );
  }
  if (item.model_inference_reached_clipboard_write !== true) {
    failures.push(
      "golden-dictation clipboard-mutation did not reach the clipboard barrier.",
    );
  }
  if (item.synthetic_clipboard_mutation_preserved !== true) {
    failures.push(
      "golden-dictation clipboard-mutation did not preserve the newer clipboard value.",
    );
  }
  if (item.clipboard_precondition_empty !== true) {
    failures.push(
      "golden-dictation clipboard-mutation did not establish an empty clipboard before testing.",
    );
  }
  if (item.controlled_target_unchanged !== true) {
    failures.push(
      "golden-dictation clipboard-mutation changed the controlled target.",
    );
  }
  validateGoldenInsertion(
    item.insertion,
    "golden-dictation clipboard-mutation insertion",
    {
      attempted: true,
      succeeded: false,
      targetVerified: true,
      error: "clipboard changed before paste",
    },
  );
}

function validateGoldenInsertion(
  insertion: GoldenDictationInsertionReceipt | undefined,
  label: string,
  expected: {
    attempted: boolean;
    succeeded: boolean;
    targetVerified: boolean;
    error: string | null;
  },
): void {
  if (!insertion) {
    failures.push(`${label} is missing.`);
    return;
  }
  requireExactKeys(
    insertion,
    ["attempted", "succeeded", "target_verified", "error"],
    label,
  );
  if (insertion.attempted !== expected.attempted) {
    failures.push(`${label} attempted did not match the expected value.`);
  }
  if (insertion.succeeded !== expected.succeeded) {
    failures.push(`${label} succeeded did not match the expected value.`);
  }
  if (insertion.target_verified !== expected.targetVerified) {
    failures.push(`${label} target_verified did not match the expected value.`);
  }
  if (insertion.error !== expected.error) {
    failures.push(`${label} error did not match the expected reason code.`);
  }
}

function validateGoldenPlaybackArtifacts(outputDeviceName: string): void {
  for (const relativePath of [
    "stable-focus.playback.json",
    "focus-switch.playback.json",
    "clipboard-mutation.playback.json",
  ]) {
    const playback = readJson<Record<string, unknown>>(relativePath);
    if (!playback) continue;
    requireExactKeys(
      playback,
      [
        "checked_device",
        "device_found",
        "stream_started",
        "success",
        "device_name",
        "source_sample_rate",
        "source_channels",
        "source_frames",
        "submitted_frames",
        "failure_class",
      ],
      relativePath,
    );
    if (playback.checked_device !== true || playback.device_found !== true) {
      failures.push(`${relativePath} did not select an active render device.`);
    }
    if (playback.stream_started !== true || playback.success !== true) {
      failures.push(`${relativePath} did not complete WASAPI playback.`);
    }
    if (playback.device_name !== outputDeviceName) {
      failures.push(`${relativePath} used an unexpected render device.`);
    }
    if (
      playback.source_sample_rate !== 16000 ||
      playback.source_channels !== 1
    ) {
      failures.push(`${relativePath} must render a 16 kHz mono fixture.`);
    }
    if (
      !positiveInteger(playback.source_frames) ||
      playback.submitted_frames !== playback.source_frames
    ) {
      failures.push(`${relativePath} did not submit the complete fixture.`);
    }
    if (playback.failure_class !== null) {
      failures.push(`${relativePath} failure_class must be null.`);
    }
  }
}

function validateGoldenReceiptArtifact(
  relativePath: string,
  expectedCase: string,
  expected: {
    attempted: boolean;
    succeeded: boolean;
    targetVerified: boolean;
    error: string | null;
  },
): void {
  const fullPath = join(artifactDir, relativePath);
  if (!existsSync(fullPath)) {
    failures.push(`${relativePath} is missing.`);
    return;
  }

  let lines: string[];
  try {
    lines = readFileSync(fullPath, "utf8")
      .split(/\r?\n/)
      .filter((line) => line.trim().length > 0);
  } catch (error) {
    failures.push(`${relativePath} could not be read: ${error}`);
    return;
  }
  if (lines.length !== 1) {
    failures.push(`${relativePath} must contain exactly one JSONL receipt.`);
    return;
  }

  let receipt: Record<string, unknown>;
  try {
    receipt = JSON.parse(lines[0]) as Record<string, unknown>;
  } catch (error) {
    failures.push(`${relativePath} is not valid JSONL: ${error}`);
    return;
  }
  requireExactKeys(
    receipt,
    [
      "schema_version",
      "case",
      "attempted",
      "succeeded",
      "method",
      "target_verified",
      "error",
    ],
    relativePath,
  );
  if (receipt.schema_version !== 1 || receipt.case !== expectedCase) {
    failures.push(`${relativePath} has an unexpected receipt identity.`);
  }
  if (!nonEmptyString(receipt.method)) {
    failures.push(`${relativePath} insertion method is missing.`);
  }
  validateGoldenInsertion(
    {
      attempted: receipt.attempted as boolean | undefined,
      succeeded: receipt.succeeded as boolean | undefined,
      target_verified: receipt.target_verified as boolean | undefined,
      error: receipt.error as string | null | undefined,
    },
    relativePath,
    expected,
  );
}

function requireExactKeys(
  value: object,
  expectedKeys: string[],
  label: string,
): void {
  const actualKeys = Object.keys(value);
  const unexpected = actualKeys.filter((key) => !expectedKeys.includes(key));
  const missing = expectedKeys.filter((key) => !actualKeys.includes(key));
  if (unexpected.length > 0) {
    failures.push(
      `${label} includes unexpected field(s): ${unexpected.join(", ")}.`,
    );
  }
  if (missing.length > 0) {
    failures.push(`${label} is missing field(s): ${missing.join(", ")}.`);
  }
}

function nonEmptyString(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0;
}

function positiveInteger(value: unknown): boolean {
  return typeof value === "number" && Number.isInteger(value) && value > 0;
}
