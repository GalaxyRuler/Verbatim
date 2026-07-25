import { spawn } from "node:child_process";
import {
  createWriteStream,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { platform } from "node:os";

type SmokePhase = {
  name: string;
  args: string[];
  timeoutMs: number;
  smokeExitMs: number;
  env?: Record<string, string>;
};

type ProcessResult = {
  code: number | null;
  signal: NodeJS.Signals | null;
  durationMs: number;
};

type NativeSmokeStatus = {
  startup_status?:
    | { status?: string; step?: string; message?: string }
    | string;
  settings_loaded?: boolean;
  main_window_created?: boolean;
  tray_initialized?: boolean;
  tray_visible_requested?: boolean;
  no_tray_cli?: boolean;
  updater_plugin_registered?: boolean;
  single_instance_plugin_registered?: boolean;
  close_to_tray_handler_registered?: boolean;
  debug_mode_enabled?: boolean;
  selected_microphone?: string;
  selected_model_configured?: boolean;
  selected_model_id?: string;
  selected_model_downloaded?: boolean;
  selected_model_custom?: boolean;
  selected_model_has_remote_url?: boolean;
  coordinator_health_events?: Array<{
    status?: string;
    restart_count?: number;
    reason?: string;
  }>;
  audio_fixture_path?: string | null;
  audio_fixture_sample_count?: number;
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
  resource_probe_failures?: string[];
  retention?: {
    history_enabled?: boolean;
    recordings_enabled?: boolean;
    history_limit?: number;
    recording_retention_period?: string;
    history_entry_count?: number;
    recording_file_count?: number;
    storage_policy_drill_verified?: boolean;
    storage_policy_drill?: Array<{
      case?: string;
      history_enabled?: boolean;
      recordings_enabled?: boolean;
      expected_history_enabled?: boolean;
      expected_recordings_enabled?: boolean;
      passed?: boolean;
    }>;
    clean_profile_verified?: boolean;
    failures?: string[];
  } | null;
  linux_environment?: {
    is_linux?: boolean;
    session_type?: string;
    desktop?: string;
    is_wayland?: boolean;
    is_x11?: boolean;
    helpers?: Array<{
      name?: string;
      available?: boolean;
      roles?: string[];
    }>;
    clipboard_helper?: string | null;
    key_combo_helper?: string | null;
    direct_input_helper?: string | null;
    at_spi_available?: boolean;
    tray_status?: string;
    warnings?: string[];
  };
  credential_store?: {
    available?: boolean;
    platform?: string;
    message?: string | null;
    retained_legacy_api_key_count?: number;
  };
  credential_migration?: {
    checked?: boolean;
    skipped?: boolean;
    available?: boolean;
    retained_legacy_api_key_count?: number;
    legacy_key_removed_from_settings?: boolean;
    credential_round_trip_verified?: boolean;
    cleanup_succeeded?: boolean;
    leaked_probe_secret?: boolean;
    failures?: string[];
  } | null;
  model_load_fallback_drill?: Array<{
    case?: string;
    diagnostic_code?: string;
    retry_on_cpu?: boolean;
    expected_retry_on_cpu?: boolean;
    success_fallback?: string | null;
    passed?: boolean;
  }>;
  insertion_safety_drill?: Array<{
    case?: string;
    paste_callback_invoked?: boolean;
    attempted?: boolean;
    target_verified?: boolean;
    error?: string | null;
    passed?: boolean;
  }>;
  clipboard_safety_drill?: Array<{
    case?: string;
    owned_by_verbatim?: boolean;
    expected_owned_by_verbatim?: boolean;
    passed?: boolean;
  }>;
};

type AppInsertionDrillCase = {
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
};

type AppInsertionDrillEvidence = {
  schema_version?: number;
  cases?: AppInsertionDrillCase[];
};

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
const tauriRoot = join(repoRoot, "src-tauri");
const hostPlatform = platform();

const args = process.argv.slice(2);

if (args.includes("--help") || args.includes("-h")) {
  console.log(`Usage: bun run smoke:native -- [options]

Options:
  --app <path>              Packaged executable path.
  --artifact-dir <path>     Directory for logs, screenshots, and summary JSON.
  --timeout-ms <number>     Per-launch timeout in milliseconds.
  --skip-single-instance    Skip duplicate-process single-instance smoke.
  --skip-startup-failure    Skip forced startup-failure recovery smoke.
  --skip-coordinator-panic  Skip forced coordinator panic supervision smoke.
  --smoke-microphone <name> Select a microphone by name for native smoke.
  --real-inference-wav <path> Run an opt-in local model inference from this WAV.
  --real-inference-model <id> Model ID used by --real-inference-wav.
  --real-inference-model-dir <path>
                            Disposable directory containing that model.
  --require-real-inference  Fail if the requested real local inference is not proven.
  --desktop-target-drill    Run controlled OS text-target and clipboard drill.
  --require-desktop-target  Fail if the controlled desktop target is unavailable.
  --allow-text-entry        Allow typing a synthetic marker into the target.
  --allow-clipboard-write   Allow writing a synthetic marker to the OS clipboard.
  --app-insertion-drills <path>
                            Validate full app-driven insertion race evidence JSON.
  --require-app-insertion-drills
                            Fail if app-driven insertion race evidence is missing.
  --help                   Show this help text.
`);
  process.exit(0);
}

function argValue(name: string): string | undefined {
  const index = args.indexOf(name);
  if (index >= 0) return args[index + 1];
  const prefix = `${name}=`;
  return args.find((arg) => arg.startsWith(prefix))?.slice(prefix.length);
}

function hasArg(name: string): boolean {
  return args.includes(name);
}

const artifactDir = resolve(
  argValue("--artifact-dir") ??
    process.env.VERBATIM_SMOKE_ARTIFACT_DIR ??
    join(repoRoot, "native-smoke-artifacts"),
);
const isolatedRoot = join(artifactDir, "isolated-profile");
const appPath = resolveAppPath(
  argValue("--app") ?? process.env.VERBATIM_SMOKE_APP_PATH,
);
const timeoutMs = Number(argValue("--timeout-ms") ?? "30000");
const skipSingleInstance = hasArg("--skip-single-instance");
const skipStartupFailure = hasArg("--skip-startup-failure");
const skipCoordinatorPanic = hasArg("--skip-coordinator-panic");
const expectedSmokeMicrophone =
  argValue("--smoke-microphone") ??
  process.env.VERBATIM_SMOKE_SELECTED_MICROPHONE;
const realInferenceWavArg =
  argValue("--real-inference-wav") ??
  process.env.VERBATIM_SMOKE_REAL_INFERENCE_WAV;
const realInferenceModel =
  argValue("--real-inference-model") ??
  process.env.VERBATIM_SMOKE_SELECTED_MODEL;
const realInferenceModelDirArg =
  argValue("--real-inference-model-dir") ??
  process.env.VERBATIM_SMOKE_MODEL_DIR;
const requireRealInference = hasArg("--require-real-inference");
const desktopTargetDrill = hasArg("--desktop-target-drill");
const requireDesktopTarget = hasArg("--require-desktop-target");
const allowTextEntry = hasArg("--allow-text-entry");
const allowClipboardWrite = hasArg("--allow-clipboard-write");
const appInsertionDrillsPath = resolve(
  argValue("--app-insertion-drills") ??
    join(artifactDir, "app-insertion-drills.json"),
);
const requireAppInsertionDrills = hasArg("--require-app-insertion-drills");

const realInferenceWav = requiredRealInferencePath(
  realInferenceWavArg,
  "--real-inference-wav",
);
const realInferenceModelDir = requiredRealInferencePath(
  realInferenceModelDirArg,
  "--real-inference-model-dir",
);
if (requireRealInference && !realInferenceWav) {
  throw new Error(
    "--require-real-inference needs --real-inference-wav, --real-inference-model, and --real-inference-model-dir",
  );
}
if (
  realInferenceWav &&
  (!realInferenceModel?.trim() || !realInferenceModelDir)
) {
  throw new Error(
    "--real-inference-wav needs --real-inference-model and --real-inference-model-dir",
  );
}

mkdirSync(artifactDir, { recursive: true });

const summary: Record<string, unknown> = {
  appPath,
  artifactDir,
  platform: hostPlatform,
  smokeModelId: "verbatim-smoke-model",
  phases: [],
};

assertFrontendAssetGraph();
await captureScreenshot("before");

const boot = await runPhase({
  name: "first-launch",
  args: ["--debug", "--no-tray"],
  timeoutMs,
  smokeExitMs: 2000,
});
expectCleanExit("first-launch", boot);
assertSmokeStatus("first-launch", {
  requireTrayVisible: false,
});

if (realInferenceWav && realInferenceModel && realInferenceModelDir) {
  const realInference = await runPhase({
    name: "real-inference",
    args: ["--debug", "--no-tray"],
    // Model loading is intentionally bounded in the app. Leave
    // enough room for an isolated packaged process to start and exit cleanly
    // without silently downgrading the proof to the fixture-only smoke.
    timeoutMs: Math.max(timeoutMs, 90000),
    smokeExitMs: 2000,
    env: {
      VERBATIM_SMOKE_MODEL_FIXTURE: "0",
      VERBATIM_SMOKE_REAL_INFERENCE_WAV: realInferenceWav,
      VERBATIM_SMOKE_REAL_INFERENCE_TIMEOUT_MS: String(
        Math.min(Math.max(timeoutMs, 90000), 300000),
      ),
      VERBATIM_SMOKE_SELECTED_MODEL: realInferenceModel.trim(),
      VERBATIM_SMOKE_MODEL_DIR: realInferenceModelDir,
    },
  });
  expectCleanExit("real-inference", realInference);
  assertSmokeStatus("real-inference", {
    requireTrayVisible: false,
    expectedSelectedModelId: realInferenceModel.trim(),
    expectedSelectedModelCustom: false,
    expectedSelectedModelHasCatalogUrl: true,
    requireRealInference: true,
    expectedRealInferenceModel: realInferenceModel.trim(),
  });
}

if (!skipStartupFailure) {
  const startupFailure = await runPhase({
    name: "startup-failure",
    args: ["--debug", "--no-tray"],
    timeoutMs,
    smokeExitMs: 2000,
    env: {
      VERBATIM_SMOKE_FORCE_STARTUP_FAILURE: "1",
    },
  });
  expectCleanExit("startup-failure", startupFailure);
  assertStartupFailureStatus("startup-failure");
}

if (!skipCoordinatorPanic) {
  const coordinatorPanic = await runPhase({
    name: "coordinator-panic",
    args: ["--debug", "--no-tray"],
    timeoutMs,
    smokeExitMs: 2000,
    env: {
      VERBATIM_SMOKE_COORDINATOR_PANIC_DRILL: "1",
    },
  });
  expectCleanExit("coordinator-panic", coordinatorPanic);
  assertSmokeStatus("coordinator-panic", {
    requireTrayVisible: false,
  });
  assertCoordinatorPanicStatus("coordinator-panic");
}

if (!skipSingleInstance) {
  await runSingleInstancePhase();
}

if (desktopTargetDrill) {
  await runDesktopTargetDrill();
}

assertAppInsertionDrillEvidence();
await captureScreenshot("after");
writeSummary();

function resolveAppPath(explicit: string | undefined): string {
  if (explicit) {
    const resolved = resolve(explicit);
    if (!existsSync(resolved)) {
      throw new Error(`Configured app path does not exist: ${resolved}`);
    }
    return resolved;
  }

  const targetDir = resolve(
    process.env.CARGO_TARGET_DIR ?? join(tauriRoot, "target"),
  );
  const candidates =
    hostPlatform === "win32"
      ? [
          // Cargo package name is "verbatim", so the raw binary is verbatim.exe;
          // "Verbatim"/"verbatim-app" kept for productName/legacy layouts.
          join(targetDir, "release", "verbatim.exe"),
          join(targetDir, "release", "Verbatim.exe"),
          join(targetDir, "release", "verbatim-app.exe"),
          join(targetDir, "x86_64-pc-windows-msvc", "release", "verbatim.exe"),
          join(targetDir, "x86_64-pc-windows-msvc", "release", "Verbatim.exe"),
          join(
            targetDir,
            "x86_64-pc-windows-msvc",
            "release",
            "verbatim-app.exe",
          ),
        ]
      : hostPlatform === "darwin"
        ? [
            join(
              targetDir,
              "aarch64-apple-darwin",
              "release",
              "bundle",
              "macos",
              "Verbatim.app",
              "Contents",
              "MacOS",
              "Verbatim",
            ),
            join(
              targetDir,
              "release",
              "bundle",
              "macos",
              "Verbatim.app",
              "Contents",
              "MacOS",
              "Verbatim",
            ),
            join(targetDir, "aarch64-apple-darwin", "release", "verbatim"),
            join(targetDir, "aarch64-apple-darwin", "release", "verbatim-app"),
            join(targetDir, "release", "verbatim"),
            join(targetDir, "release", "verbatim-app"),
          ]
        : [
            // Cargo package name is "verbatim", so the raw binary is "verbatim".
            join(targetDir, "release", "verbatim"),
            join(targetDir, "release", "verbatim-app"),
            join(targetDir, "release", "Verbatim"),
            join(targetDir, "x86_64-unknown-linux-gnu", "release", "verbatim"),
            join(
              targetDir,
              "x86_64-unknown-linux-gnu",
              "release",
              "verbatim-app",
            ),
            join(targetDir, "x86_64-unknown-linux-gnu", "release", "Verbatim"),
          ];

  const found = candidates.find((candidate) => existsSync(candidate));
  if (found) return found;

  throw new Error(
    `Unable to locate packaged executable. Pass --app or VERBATIM_SMOKE_APP_PATH. Checked: ${candidates.join(", ")}`,
  );
}

function requiredRealInferencePath(
  value: string | undefined,
  option: string,
): string | undefined {
  if (!value?.trim()) return undefined;
  const resolved = resolve(value);
  if (!existsSync(resolved)) {
    throw new Error(`${option} does not exist: ${resolved}`);
  }
  return resolved;
}

async function runSingleInstancePhase(): Promise<void> {
  const primaryLogPrefix = "single-instance-primary";
  const secondaryLogPrefix = "single-instance-secondary";
  const primary = spawnApp(["--debug", "--no-tray"], primaryLogPrefix, 4500);

  await delay(1000);
  const secondary = await runSpawnedProcess(
    spawnApp(["--debug", "--no-tray"], secondaryLogPrefix, 1000),
    secondaryLogPrefix,
    10000,
  );
  expectCleanExit(secondaryLogPrefix, secondary);
  assertNoSmokeStatus(secondaryLogPrefix);

  // The primary self-exits VERBATIM_SMOKE_EXIT_AFTER_MS (4500ms) after setup
  // completes, but setup itself can take >5s on loaded CI Windows runners
  // (Defender scanning a freshly written exe, cold model-fixture load), which
  // made a 10s wall flake. 30s still fails fast on a genuine hang.
  const primaryResult = await runSpawnedProcess(
    primary,
    primaryLogPrefix,
    30000,
  );
  expectCleanExit(primaryLogPrefix, primaryResult);
  assertSmokeStatus(primaryLogPrefix, {
    requireTrayVisible: false,
  });
}

function assertNoSmokeStatus(logPrefix: string): void {
  const statusPath = join(artifactDir, `${logPrefix}.status.json`);
  if (existsSync(statusPath)) {
    throw new Error(
      `${logPrefix} wrote native smoke status, so the duplicate process reached app setup instead of delegating to the primary instance: ${statusPath}`,
    );
  }
}

async function runDesktopTargetDrill(): Promise<void> {
  const script = join(
    repoRoot,
    "scripts",
    "native-smoke",
    "controlled-desktop-targets.ts",
  );
  const drillArgs = [script, "--artifact-dir", artifactDir];
  if (requireDesktopTarget) {
    drillArgs.push("--require");
  }
  if (allowTextEntry) {
    drillArgs.push("--allow-text-entry");
  }
  if (allowClipboardWrite) {
    drillArgs.push("--allow-clipboard-write");
  }

  const result = await runSpawnedProcess(
    spawn(process.execPath, drillArgs, {
      cwd: repoRoot,
      env: process.env,
      stdio: ["ignore", "pipe", "pipe"],
      windowsHide: true,
    }),
    "controlled-desktop-targets",
    30000,
  );
  expectCleanExit("controlled-desktop-targets", result);

  const outputPath = join(artifactDir, "controlled-desktop-targets.json");
  if (!existsSync(outputPath)) {
    throw new Error(
      `controlled desktop target drill did not write ${outputPath}`,
    );
  }

  (summary as Record<string, unknown>).controlledDesktopTargets = JSON.parse(
    readFileSync(outputPath, "utf8"),
  );
  writeSummary();
}

async function runPhase(phase: SmokePhase): Promise<ProcessResult> {
  const child = spawnApp(phase.args, phase.name, phase.smokeExitMs, phase.env);
  return runSpawnedProcess(child, phase.name, phase.timeoutMs);
}

function spawnApp(
  appArgs: string[],
  logPrefix: string,
  smokeExitMs: number,
  extraEnv?: Record<string, string>,
) {
  const stdout = createWriteStream(
    join(artifactDir, `${logPrefix}.stdout.log`),
  );
  const stderr = createWriteStream(
    join(artifactDir, `${logPrefix}.stderr.log`),
  );
  const child = spawn(appPath, appArgs, {
    cwd: dirname(appPath),
    env: smokeEnvForPhase(smokeExitMs, logPrefix, extraEnv),
    stdio: ["ignore", "pipe", "pipe"],
    windowsHide: true,
  });

  child.stdout?.pipe(stdout);
  child.stderr?.pipe(stderr);
  return child;
}

function smokeEnvForPhase(
  smokeExitMs: number,
  logPrefix: string,
  extraEnv: Record<string, string> = {},
): NodeJS.ProcessEnv {
  const env: NodeJS.ProcessEnv = {
    ...process.env,
    RUST_LOG: process.env.RUST_LOG ?? "trace",
    VERBATIM_SMOKE_AUDIO_FIXTURE_PATH: join(
      artifactDir,
      `${logPrefix}.fixture.wav`,
    ),
    VERBATIM_SMOKE_EXIT_AFTER_MS: String(smokeExitMs),
    VERBATIM_SMOKE_MODEL_FIXTURE: "1",
    VERBATIM_SMOKE_STATUS_PATH: join(artifactDir, `${logPrefix}.status.json`),
    VERBATIM_SMOKE_DATA_DIR: join(isolatedRoot, `${logPrefix}-data`),
    VERBATIM_NO_GTK_LAYER_SHELL: "1",
    XDG_CONFIG_HOME: join(isolatedRoot, "config"),
    XDG_DATA_HOME: join(isolatedRoot, "data"),
    XDG_CACHE_HOME: join(isolatedRoot, "cache"),
    ...extraEnv,
  };

  if (expectedSmokeMicrophone) {
    env.VERBATIM_SMOKE_SELECTED_MICROPHONE = expectedSmokeMicrophone;
  }

  if (hostPlatform === "win32") {
    env.APPDATA = join(isolatedRoot, "AppData", "Roaming");
    env.LOCALAPPDATA = join(isolatedRoot, "AppData", "Local");
  }

  return env;
}

function startupStatus(status: NativeSmokeStatus): {
  status?: string;
  step?: string;
  message?: string;
} {
  return typeof status.startup_status === "string"
    ? { status: status.startup_status }
    : (status.startup_status ?? {});
}

function assertSmokeStatus(
  logPrefix: string,
  options: {
    requireTrayVisible: boolean;
    expectedSelectedModelId?: string;
    expectedSelectedModelCustom?: boolean;
    expectedSelectedModelHasCatalogUrl?: boolean;
    requireRealInference?: boolean;
    expectedRealInferenceModel?: string;
  },
): void {
  const statusPath = join(artifactDir, `${logPrefix}.status.json`);
  if (!existsSync(statusPath)) {
    throw new Error(
      `${logPrefix} did not write native smoke status: ${statusPath}`,
    );
  }

  const status = JSON.parse(
    readFileSync(statusPath, "utf8"),
  ) as NativeSmokeStatus;
  const startup = startupStatus(status);
  const expectedSelectedModelId =
    options.expectedSelectedModelId ?? "verbatim-smoke-model";
  const expectedSelectedModelCustom =
    options.expectedSelectedModelCustom ?? true;
  const expectedSelectedModelHasCatalogUrl =
    options.expectedSelectedModelHasCatalogUrl ?? false;

  const failures = [
    startup.status === "ready"
      ? null
      : `startup_status=${String(startup.status)}`,
    status.settings_loaded ? null : "settings_loaded=false",
    status.main_window_created ? null : "main_window_created=false",
    status.tray_initialized ? null : "tray_initialized=false",
    status.updater_plugin_registered ? null : "updater_plugin_registered=false",
    status.single_instance_plugin_registered
      ? null
      : "single_instance_plugin_registered=false",
    status.close_to_tray_handler_registered
      ? null
      : "close_to_tray_handler_registered=false",
    status.debug_mode_enabled ? null : "debug_mode_enabled=false",
    selectedMicrophoneFailure(status),
    status.selected_model_configured ? null : "selected_model_configured=false",
    status.selected_model_id === expectedSelectedModelId
      ? null
      : `selected_model_id=${String(status.selected_model_id)}`,
    status.selected_model_downloaded ? null : "selected_model_downloaded=false",
    status.selected_model_custom === expectedSelectedModelCustom
      ? null
      : `selected_model_custom=${String(status.selected_model_custom)}`,
    status.selected_model_has_remote_url === expectedSelectedModelHasCatalogUrl
      ? null
      : `selected_model_has_remote_url=${String(status.selected_model_has_remote_url)}`,
    status.audio_fixture_path ? null : "audio_fixture_path=missing",
    status.audio_fixture_sample_count === 32000
      ? null
      : `audio_fixture_sample_count=${String(status.audio_fixture_sample_count)}`,
    status.audio_fixture_verified ? null : "audio_fixture_verified=false",
    status.audio_fixture_path && existsSync(status.audio_fixture_path)
      ? null
      : `audio_fixture_file_missing=${String(status.audio_fixture_path)}`,
    status.resource_probe_checked ? null : "resource_probe_checked=false",
    (status.resource_probe_failures ?? []).length === 0
      ? null
      : `resource_probe_failures=${(status.resource_probe_failures ?? []).join("; ")}`,
    status.retention ? null : "retention=missing",
    status.retention?.history_enabled
      ? null
      : `retention.history_enabled=${String(status.retention?.history_enabled)}`,
    status.retention?.recordings_enabled
      ? null
      : `retention.recordings_enabled=${String(status.retention?.recordings_enabled)}`,
    status.retention?.history_limit === 5
      ? null
      : `retention.history_limit=${String(status.retention?.history_limit)}`,
    status.retention?.recording_retention_period === "preserve_limit"
      ? null
      : `retention.recording_retention_period=${String(status.retention?.recording_retention_period)}`,
    status.retention?.history_entry_count === 0
      ? null
      : `retention.history_entry_count=${String(status.retention?.history_entry_count)}`,
    status.retention?.recording_file_count === 0
      ? null
      : `retention.recording_file_count=${String(status.retention?.recording_file_count)}`,
    status.retention?.storage_policy_drill_verified
      ? null
      : `retention.storage_policy_drill_verified=${String(status.retention?.storage_policy_drill_verified)}`,
    ...storagePolicyDrillFailures(status.retention?.storage_policy_drill),
    status.retention?.clean_profile_verified
      ? null
      : `retention.clean_profile_verified=${String(status.retention?.clean_profile_verified)}`,
    (status.retention?.failures ?? []).length === 0
      ? null
      : `retention.failures=${(status.retention?.failures ?? []).join("; ")}`,
    status.credential_store ? null : "credential_store=missing",
    typeof status.credential_store?.platform === "string" &&
    status.credential_store.platform.length > 0
      ? null
      : `credential_store.platform=${String(status.credential_store?.platform)}`,
    status.credential_store?.retained_legacy_api_key_count === 0
      ? null
      : `credential_store.retained_legacy_api_key_count=${String(status.credential_store?.retained_legacy_api_key_count)}`,
    status.credential_store?.message?.includes("__verbatim_health_probe__")
      ? "credential_store.message_leaked_probe_secret=true"
      : null,
    ...credentialMigrationFailures(status.credential_migration),
    ...modelLoadFallbackDrillFailures(status.model_load_fallback_drill),
    ...insertionSafetyDrillFailures(status.insertion_safety_drill),
    ...clipboardSafetyDrillFailures(status.clipboard_safety_drill),
    ...linuxEnvironmentFailures(status),
    ...realInferenceFailures(
      status.real_inference,
      options.requireRealInference ?? false,
      options.expectedRealInferenceModel,
    ),
    options.requireTrayVisible && !status.tray_visible_requested
      ? "tray_visible_requested=false"
      : null,
  ].filter(Boolean);

  if (failures.length > 0) {
    throw new Error(
      `${logPrefix} native smoke status failed: ${failures.join(", ")}`,
    );
  }

  (summary as Record<string, unknown[]>).statuses ??= [];
  (summary as Record<string, unknown[]>).statuses.push({
    name: logPrefix,
    status,
  });
  writeSummary();
}

function realInferenceFailures(
  inference: NativeSmokeStatus["real_inference"],
  required: boolean,
  expectedModel: string | undefined,
): string[] {
  if (!required) return [];
  if (!inference) return ["real_inference=missing"];

  return [
    inference.checked ? null : "real_inference.checked=false",
    expectedModel && inference.model_id !== expectedModel
      ? `real_inference.model_id=${String(inference.model_id)}`
      : null,
    (inference.audio_sample_count ?? 0) > 0
      ? null
      : `real_inference.audio_sample_count=${String(inference.audio_sample_count)}`,
    inference.model_loaded ? null : "real_inference.model_loaded=false",
    inference.inference_started
      ? null
      : "real_inference.inference_started=false",
    inference.inference_completed
      ? null
      : "real_inference.inference_completed=false",
    inference.transcript_non_empty
      ? null
      : "real_inference.transcript_non_empty=false",
    inference.transcript_recorded
      ? "real_inference.transcript_recorded=true"
      : null,
    inference.failure_class
      ? `real_inference.failure_class=${inference.failure_class}`
      : null,
  ].filter((failure): failure is string => Boolean(failure));
}

function selectedMicrophoneFailure(status: NativeSmokeStatus): string | null {
  if (!expectedSmokeMicrophone) return null;

  const expected = expectedSmokeMicrophone.trim();
  if (expected.length === 0) return null;
  const normalizedExpected =
    expected.toLowerCase() === "default" ? "default" : expected;

  return status.selected_microphone === normalizedExpected
    ? null
    : `selected_microphone=${String(status.selected_microphone)}`;
}

function clipboardSafetyDrillFailures(
  drill: NativeSmokeStatus["clipboard_safety_drill"],
): string[] {
  if (!Array.isArray(drill)) {
    return ["clipboard_safety_drill=missing"];
  }

  const expectedCases = new Map<string, boolean>([
    ["same_text_sequence_changed", false],
    ["changed_text_matching_sequence", false],
    ["exact_text_without_sequence", true],
  ]);
  const failures: string[] = [];

  for (const [caseName, expectedOwned] of expectedCases) {
    const actual = drill.find((item) => item.case === caseName);
    if (!actual) {
      failures.push(`clipboard_safety_drill.${caseName}=missing`);
      continue;
    }
    if (!actual.passed) {
      failures.push(`clipboard_safety_drill.${caseName}.passed=false`);
    }
    if (actual.expected_owned_by_verbatim !== expectedOwned) {
      failures.push(
        `clipboard_safety_drill.${caseName}.expected_owned_by_verbatim=${String(actual.expected_owned_by_verbatim)}`,
      );
    }
    if (actual.owned_by_verbatim !== expectedOwned) {
      failures.push(
        `clipboard_safety_drill.${caseName}.owned_by_verbatim=${String(actual.owned_by_verbatim)}`,
      );
    }
  }

  return failures;
}

function insertionSafetyDrillFailures(
  drill: NativeSmokeStatus["insertion_safety_drill"],
): string[] {
  if (!Array.isArray(drill)) {
    return ["insertion_safety_drill=missing"];
  }

  const expectedCases = [
    "adaptive_target_changed_blocks_paste",
    "classic_target_changed_blocks_paste",
  ];
  const failures: string[] = [];

  for (const caseName of expectedCases) {
    const actual = drill.find((item) => item.case === caseName);
    if (!actual) {
      failures.push(`insertion_safety_drill.${caseName}=missing`);
      continue;
    }
    if (!actual.passed) {
      failures.push(`insertion_safety_drill.${caseName}.passed=false`);
    }
    if (actual.paste_callback_invoked) {
      failures.push(
        `insertion_safety_drill.${caseName}.paste_callback_invoked=true`,
      );
    }
    if (actual.attempted) {
      failures.push(`insertion_safety_drill.${caseName}.attempted=true`);
    }
    if (actual.target_verified) {
      failures.push(`insertion_safety_drill.${caseName}.target_verified=true`);
    }
    if (actual.error !== "target changed before insertion") {
      failures.push(
        `insertion_safety_drill.${caseName}.error=${String(actual.error)}`,
      );
    }
  }

  return failures;
}

function credentialMigrationFailures(
  migration: NativeSmokeStatus["credential_migration"],
): string[] {
  if (!migration) return ["credential_migration=missing"];

  const failures = [
    migration.checked ? null : "credential_migration.checked=false",
    migration.leaked_probe_secret
      ? "credential_migration.leaked_probe_secret=true"
      : null,
    ...(migration.failures ?? []).map(
      (failure) => `credential_migration.failure=${failure}`,
    ),
  ].filter((failure): failure is string => Boolean(failure));

  if (migration.skipped) {
    if (migration.available) {
      failures.push("credential_migration.skipped_while_available=true");
    }
    return failures;
  }

  return [
    ...failures,
    migration.available ? null : "credential_migration.available=false",
    migration.retained_legacy_api_key_count === 0
      ? null
      : `credential_migration.retained_legacy_api_key_count=${String(migration.retained_legacy_api_key_count)}`,
    migration.legacy_key_removed_from_settings
      ? null
      : "credential_migration.legacy_key_removed_from_settings=false",
    migration.credential_round_trip_verified
      ? null
      : "credential_migration.credential_round_trip_verified=false",
    migration.cleanup_succeeded
      ? null
      : "credential_migration.cleanup_succeeded=false",
  ].filter((failure): failure is string => Boolean(failure));
}

function modelLoadFallbackDrillFailures(
  drill: NativeSmokeStatus["model_load_fallback_drill"],
): string[] {
  if (!Array.isArray(drill)) {
    return ["model_load_fallback_drill=missing"];
  }

  const expectedCases = new Map<
    string,
    {
      retry_on_cpu: boolean;
      diagnostic_code: string;
      success_fallback: string | null;
    }
  >([
    [
      "whisper_gpu_accelerator_failure",
      {
        retry_on_cpu: true,
        diagnostic_code: "accelerator_load_failed",
        success_fallback: "cpu_after_accelerator_load_failed",
      },
    ],
    [
      "whisper_cpu_accelerator_failure",
      {
        retry_on_cpu: false,
        diagnostic_code: "accelerator_load_failed",
        success_fallback: null,
      },
    ],
    [
      "ort_directml_accelerator_failure",
      {
        retry_on_cpu: true,
        diagnostic_code: "accelerator_load_failed",
        success_fallback: "cpu_after_accelerator_load_failed",
      },
    ],
    [
      "generic_provider_failure",
      {
        retry_on_cpu: false,
        diagnostic_code: "provider_load_failed",
        success_fallback: null,
      },
    ],
  ]);

  const failures: string[] = [];
  for (const [caseName, expected] of expectedCases) {
    const actual = drill.find((item) => item.case === caseName);
    if (!actual) {
      failures.push(`model_load_fallback_drill.${caseName}=missing`);
      continue;
    }
    if (!actual.passed) {
      failures.push(`model_load_fallback_drill.${caseName}.passed=false`);
    }
    if (actual.retry_on_cpu !== expected.retry_on_cpu) {
      failures.push(
        `model_load_fallback_drill.${caseName}.retry_on_cpu=${String(actual.retry_on_cpu)}`,
      );
    }
    if (actual.diagnostic_code !== expected.diagnostic_code) {
      failures.push(
        `model_load_fallback_drill.${caseName}.diagnostic_code=${String(actual.diagnostic_code)}`,
      );
    }
    if ((actual.success_fallback ?? null) !== expected.success_fallback) {
      failures.push(
        `model_load_fallback_drill.${caseName}.success_fallback=${String(actual.success_fallback)}`,
      );
    }
  }

  return failures;
}

function storagePolicyDrillFailures(
  drill: NativeSmokeStatus["retention"] extends infer Retention
    ? Retention extends { storage_policy_drill?: infer Drill }
      ? Drill
      : never
    : never,
): string[] {
  if (!Array.isArray(drill)) return ["retention.storage_policy_drill=missing"];

  const expectedCases = new Map<
    string,
    { history_enabled: boolean; recordings_enabled: boolean }
  >([
    ["default", { history_enabled: true, recordings_enabled: true }],
    [
      "recordings_disabled",
      { history_enabled: true, recordings_enabled: false },
    ],
    ["history_disabled", { history_enabled: false, recordings_enabled: false }],
    ["private_session", { history_enabled: false, recordings_enabled: false }],
  ]);

  const failures: string[] = [];
  for (const [caseName, expected] of expectedCases) {
    const actual = drill.find((item) => item.case === caseName);
    if (!actual) {
      failures.push(`retention.storage_policy_drill.${caseName}=missing`);
      continue;
    }
    if (!actual.passed) {
      failures.push(`retention.storage_policy_drill.${caseName}.passed=false`);
    }
    if (actual.history_enabled !== expected.history_enabled) {
      failures.push(
        `retention.storage_policy_drill.${caseName}.history_enabled=${String(actual.history_enabled)}`,
      );
    }
    if (actual.recordings_enabled !== expected.recordings_enabled) {
      failures.push(
        `retention.storage_policy_drill.${caseName}.recordings_enabled=${String(actual.recordings_enabled)}`,
      );
    }
  }

  return failures;
}

function linuxEnvironmentFailures(status: NativeSmokeStatus): string[] {
  const linux = status.linux_environment;
  if (hostPlatform !== "linux") {
    return linux?.is_linux ? ["linux_environment.is_linux=true"] : [];
  }

  const xdotool = linux?.helpers?.find((helper) => helper.name === "xdotool");

  return [
    linux ? null : "linux_environment=missing",
    linux?.is_linux
      ? null
      : `linux_environment.is_linux=${String(linux?.is_linux)}`,
    linux?.is_x11 || linux?.is_wayland
      ? null
      : `linux_environment.session=${String(linux?.session_type)}`,
    linux?.direct_input_helper
      ? null
      : "linux_environment.direct_input_helper=missing",
    linux?.key_combo_helper
      ? null
      : "linux_environment.key_combo_helper=missing",
    xdotool?.available
      ? null
      : `linux_environment.xdotool.available=${String(xdotool?.available)}`,
    linux?.is_x11 && linux.direct_input_helper !== "xdotool"
      ? `linux_environment.direct_input_helper=${String(linux.direct_input_helper)}`
      : null,
    linux?.is_x11 && linux.key_combo_helper !== "xdotool"
      ? `linux_environment.key_combo_helper=${String(linux.key_combo_helper)}`
      : null,
  ].filter((failure): failure is string => Boolean(failure));
}

function assertStartupFailureStatus(logPrefix: string): void {
  const statusPath = join(artifactDir, `${logPrefix}.status.json`);
  if (!existsSync(statusPath)) {
    throw new Error(
      `${logPrefix} did not write native smoke status: ${statusPath}`,
    );
  }

  const status = JSON.parse(
    readFileSync(statusPath, "utf8"),
  ) as NativeSmokeStatus;
  const startup = startupStatus(status);
  const failures = [
    startup.status === "failed"
      ? null
      : `startup_status=${String(startup.status)}`,
    startup.step === "native smoke forced startup failure"
      ? null
      : `startup_step=${String(startup.step)}`,
    startup.message ===
    "forced startup failure for packaged smoke recovery drill"
      ? null
      : `startup_message=${String(startup.message)}`,
    status.settings_loaded ? null : "settings_loaded=false",
    status.main_window_created ? null : "main_window_created=false",
    status.tray_initialized ? "tray_initialized=true" : null,
    status.close_to_tray_handler_registered
      ? "close_to_tray_handler_registered=true"
      : null,
    selectedMicrophoneFailure(status),
  ].filter(Boolean);

  if (failures.length > 0) {
    throw new Error(
      `${logPrefix} native startup-failure smoke failed: ${failures.join(", ")}`,
    );
  }

  (summary as Record<string, unknown[]>).statuses ??= [];
  (summary as Record<string, unknown[]>).statuses.push({
    name: logPrefix,
    status,
  });
  writeSummary();
}

function assertCoordinatorPanicStatus(logPrefix: string): void {
  const statusPath = join(artifactDir, `${logPrefix}.status.json`);
  if (!existsSync(statusPath)) {
    throw new Error(
      `${logPrefix} did not write native smoke status: ${statusPath}`,
    );
  }

  const status = JSON.parse(
    readFileSync(statusPath, "utf8"),
  ) as NativeSmokeStatus;
  const events = status.coordinator_health_events ?? [];
  const first = events[0];
  const second = events[1];
  const failures = [
    events.length >= 2 ? null : `coordinator_health_events=${events.length}`,
    first?.status === "restarted"
      ? null
      : `first_coordinator_status=${String(first?.status)}`,
    first?.restart_count === 1
      ? null
      : `first_restart_count=${String(first?.restart_count)}`,
    first?.reason === "worker panic"
      ? null
      : `first_reason=${String(first?.reason)}`,
    second?.status === "disabled"
      ? null
      : `second_coordinator_status=${String(second?.status)}`,
    second?.restart_count === 2
      ? null
      : `second_restart_count=${String(second?.restart_count)}`,
    second?.reason === "worker panic"
      ? null
      : `second_reason=${String(second?.reason)}`,
  ].filter(Boolean);

  if (failures.length > 0) {
    throw new Error(
      `${logPrefix} native coordinator panic smoke failed: ${failures.join(", ")}`,
    );
  }
}

function assertAppInsertionDrillEvidence(): void {
  if (!existsSync(appInsertionDrillsPath)) {
    summary.appInsertionDrills = {
      required: requireAppInsertionDrills,
      checked: false,
      evidencePath: appInsertionDrillsPath,
      note: "Full app-driven insertion race drill evidence was not provided.",
    };
    writeSummary();
    if (requireAppInsertionDrills) {
      throw new Error(
        `missing app-driven insertion race evidence: ${appInsertionDrillsPath}`,
      );
    }
    return;
  }

  const evidence = JSON.parse(
    readFileSync(appInsertionDrillsPath, "utf8"),
  ) as AppInsertionDrillEvidence;
  const failures = appInsertionDrillFailures(evidence);
  summary.appInsertionDrills = {
    required: requireAppInsertionDrills,
    checked: true,
    evidencePath: appInsertionDrillsPath,
    cases: evidence.cases ?? [],
  };
  writeSummary();

  if (failures.length > 0) {
    throw new Error(
      `app-driven insertion race evidence failed: ${failures.join(", ")}`,
    );
  }
}

function appInsertionDrillFailures(
  evidence: AppInsertionDrillEvidence,
): string[] {
  const failures = [
    evidence.schema_version === 1
      ? null
      : `schema_version=${String(evidence.schema_version)}`,
    Array.isArray(evidence.cases) ? null : "cases=missing",
  ].filter((failure): failure is string => Boolean(failure));

  if (!Array.isArray(evidence.cases)) return failures;

  const expectedCases = new Map<string, Record<string, boolean | "string">>([
    [
      "focus_switch_during_inference_blocks_insertion",
      {
        app_driven: true,
        passed: true,
        desktop_target: "string",
        inference_started: true,
        focus_switched_before_insertion: true,
        insertion_blocked: true,
      },
    ],
    [
      "clipboard_mutation_during_paste_preserves_user_clipboard",
      {
        app_driven: true,
        passed: true,
        desktop_target: "string",
        paste_attempted: true,
        clipboard_mutated_after_verbatim_write: true,
        user_clipboard_preserved: true,
        user_clipboard_contents_recorded: false,
      },
    ],
  ]);

  for (const [caseName, expectations] of expectedCases) {
    const actual = evidence.cases.find((item) => item.case === caseName);
    if (!actual) {
      failures.push(`${caseName}=missing`);
      continue;
    }
    for (const [field, expected] of Object.entries(expectations)) {
      const actualValue = actual[field as keyof AppInsertionDrillCase];
      if (expected === "string") {
        if (typeof actualValue !== "string" || actualValue.length === 0) {
          failures.push(`${caseName}.${field}=${String(actualValue)}`);
        }
        continue;
      }
      if (actualValue !== expected) {
        failures.push(`${caseName}.${field}=${String(actualValue)}`);
      }
    }
    for (const failure of actual.failures ?? []) {
      failures.push(`${caseName}.failure=${failure}`);
    }
  }

  return failures;
}

function runSpawnedProcess(
  child: ReturnType<typeof spawn>,
  name: string,
  maxMs: number,
): Promise<ProcessResult> {
  const started = Date.now();
  return new Promise((resolveProcess, rejectProcess) => {
    const timer = setTimeout(() => {
      child.kill("SIGTERM");
      rejectProcess(new Error(`${name} did not exit within ${maxMs}ms`));
    }, maxMs);

    child.on("error", (error) => {
      clearTimeout(timer);
      rejectProcess(error);
    });

    child.on("exit", (code, signal) => {
      clearTimeout(timer);
      const result = { code, signal, durationMs: Date.now() - started };
      (summary.phases as unknown[]).push({ name, ...result });
      writeSummary();
      resolveProcess(result);
    });
  });
}

function expectCleanExit(name: string, result: ProcessResult): void {
  if (result.code !== 0) {
    throw new Error(
      `${name} exited with code ${result.code} signal ${result.signal}`,
    );
  }
}

function assertFrontendAssetGraph(): void {
  const distRoot = join(repoRoot, "dist");
  const distAssets = join(distRoot, "assets");
  const indexPath = join(distRoot, "index.html");
  const overlayIndexPath = join(distRoot, "src", "overlay", "index.html");
  const localeRoot = join(repoRoot, "src", "i18n", "locales");

  const failures = [
    existsSync(indexPath) ? null : "dist/index.html is missing",
    existsSync(overlayIndexPath)
      ? null
      : "dist/src/overlay/index.html is missing",
    existsSync(distAssets) ? null : "dist/assets is missing",
  ].filter(Boolean);

  if (failures.length > 0) {
    throw new Error(
      `Frontend production asset graph is incomplete: ${failures.join(", ")}`,
    );
  }

  const indexHtml = readFileSync(indexPath, "utf8");
  const overlayHtml = readFileSync(overlayIndexPath, "utf8");
  const assets = readdirSync(distAssets).sort();
  const jsAssets = assets.filter((asset) => asset.endsWith(".js"));
  const cssAssets = assets.filter((asset) => asset.endsWith(".css"));
  const translationChunks = assets.filter((asset) =>
    /^translation-.+\.js$/.test(asset),
  );
  const localeCodes = readdirSync(localeRoot)
    .filter((entry) => existsSync(join(localeRoot, entry, "translation.json")))
    .sort();
  const expectedLazyLocaleChunks = Math.max(localeCodes.length - 1, 0);
  const assetFailures = [
    /assets\/.+\.js/.test(indexHtml)
      ? null
      : "dist/index.html does not reference a JavaScript asset",
    /assets\/.+\.css/.test(indexHtml)
      ? null
      : "dist/index.html does not reference a CSS asset",
    /assets\/.+\.js/.test(overlayHtml)
      ? null
      : "dist/src/overlay/index.html does not reference a JavaScript asset",
    jsAssets.length > 0 ? null : "dist/assets contains no JavaScript assets",
    cssAssets.length > 0 ? null : "dist/assets contains no CSS assets",
    translationChunks.length >= expectedLazyLocaleChunks
      ? null
      : `expected at least ${expectedLazyLocaleChunks} lazy locale chunks, found ${translationChunks.length}`,
  ].filter(Boolean);

  if (assetFailures.length > 0) {
    throw new Error(
      `Frontend production asset graph is incomplete: ${assetFailures.join(", ")}`,
    );
  }

  summary.frontendAssetGraph = {
    indexHtml: "dist/index.html",
    overlayIndexHtml: "dist/src/overlay/index.html",
    jsAssetCount: jsAssets.length,
    cssAssetCount: cssAssets.length,
    localeCount: localeCodes.length,
    lazyLocaleChunkCount: translationChunks.length,
  };
  writeSummary();
}

async function captureScreenshot(label: string): Promise<void> {
  const output = join(artifactDir, `${label}.png`);
  try {
    if (hostPlatform === "darwin") {
      await runUtility("screencapture", ["-x", output], 5000);
    } else if (hostPlatform === "win32") {
      const script = [
        "Add-Type -AssemblyName System.Windows.Forms",
        "Add-Type -AssemblyName System.Drawing",
        "$bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds",
        "$bitmap = New-Object System.Drawing.Bitmap $bounds.Width, $bounds.Height",
        "$graphics = [System.Drawing.Graphics]::FromImage($bitmap)",
        "$graphics.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)",
        `$bitmap.Save('${output.replaceAll("'", "''")}', [System.Drawing.Imaging.ImageFormat]::Png)`,
        "$graphics.Dispose()",
        "$bitmap.Dispose()",
      ].join("; ");
      await runUtility("powershell", ["-NoProfile", "-Command", script], 5000);
    } else {
      await runUtility("import", ["-window", "root", output], 5000);
    }
  } catch (error) {
    writeFileSync(
      join(artifactDir, `${label}-screenshot-skipped.txt`),
      String(error),
    );
  }
}

function runUtility(
  command: string,
  utilityArgs: string[],
  maxMs: number,
): Promise<void> {
  return new Promise((resolveUtility, rejectUtility) => {
    const child = spawn(command, utilityArgs, {
      stdio: "ignore",
      windowsHide: true,
    });
    const timer = setTimeout(() => {
      child.kill("SIGTERM");
      rejectUtility(new Error(`${command} timed out`));
    }, maxMs);

    child.on("error", (error) => {
      clearTimeout(timer);
      rejectUtility(error);
    });
    child.on("exit", (code) => {
      clearTimeout(timer);
      code === 0
        ? resolveUtility()
        : rejectUtility(new Error(`${command} exited with ${code}`));
    });
  });
}

function writeSummary(): void {
  summary.files = listArtifacts(artifactDir);
  writeFileSync(
    join(artifactDir, "native-smoke-summary.json"),
    JSON.stringify(summary, null, 2),
  );
}

function listArtifacts(root: string): string[] {
  if (!existsSync(root)) return [];
  const files: string[] = [];
  const walk = (current: string) => {
    let entries: string[];
    try {
      entries = readdirSync(current);
    } catch (error) {
      // WebView runtimes clean up transient files asynchronously while the
      // smoke app exits. A disappeared directory is not an artifact failure.
      if ((error as NodeJS.ErrnoException).code === "ENOENT") return;
      throw error;
    }

    for (const entry of entries) {
      const fullPath = join(current, entry);
      let stat: ReturnType<typeof statSync>;
      try {
        stat = statSync(fullPath);
      } catch (error) {
        // The directory listing is a snapshot; skip a temp file that vanishes
        // between readdirSync and statSync during WebView shutdown.
        if ((error as NodeJS.ErrnoException).code === "ENOENT") continue;
        throw error;
      }
      if (stat.isDirectory()) {
        walk(fullPath);
      } else {
        files.push(fullPath.slice(root.length + 1));
      }
    }
  };
  walk(root);
  return files.sort();
}

function delay(ms: number): Promise<void> {
  return new Promise((resolveDelay) => setTimeout(resolveDelay, ms));
}

console.log(
  `Native smoke completed for ${basename(appPath)}. Artifacts: ${artifactDir}`,
);
