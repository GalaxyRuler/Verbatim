import { execFile } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { platform } from "node:os";
import { join, resolve } from "node:path";

type CommandResult = {
  stdout: string;
  stderr: string;
};

type VirtualAudioInputResult = {
  platform: NodeJS.Platform;
  checked: boolean;
  required: boolean;
  created: boolean;
  sink_name: string | null;
  source_name: string | null;
  source_description: string | null;
  playback_device: string | null;
  module_ids: string[];
  smoke_microphone_arg: string | null;
  cleanup_commands: string[][];
  skipped_reason: string | null;
  failures: string[];
};

type VirtualAudioCleanupResult = {
  checked: boolean;
  cleanup_from: string;
  cleanup_attempted: boolean;
  cleanup_commands: string[][];
  unloaded_module_ids: string[];
  skipped_reason: string | null;
  failures: string[];
};

type VirtualAudioPlaybackResult = {
  checked: boolean;
  input_from: string;
  fixture_path: string;
  playback_attempted: boolean;
  playback_device: string | null;
  playback_command: string[] | null;
  skipped_reason: string | null;
  failures: string[];
};

const args = process.argv.slice(2);

if (args.includes("--help") || args.includes("-h")) {
  console.log(`Usage: bun scripts/native-smoke/virtual-audio-input.ts [options]

Options:
  --artifact-dir <path>          Directory for virtual-audio-input.json.
  --cleanup-from <path>          Unload modules listed in a virtual-audio-input.json file.
  --input-from <path>            Read virtual input metadata for playback.
  --play-fixture <path>          Play a WAV fixture into the virtual audio graph.
  --require                      Exit non-zero if a virtual input is unavailable.
  --device-name <name>           Use an already-provisioned virtual input name.
  --create-linux-pulse-source    Create a PulseAudio/PipeWire virtual source on Linux.
  --help                         Show this help text.

The helper does not capture user audio. On Linux, --create-linux-pulse-source
loads session-scoped PulseAudio modules and reports cleanup commands. Windows
and macOS require a preinstalled virtual audio device such as VB-CABLE or
BlackHole, passed with --device-name.
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
    join(process.cwd(), "native-smoke-artifacts"),
);
const requireVirtualInput = hasArg("--require");
const requestedDeviceName = argValue("--device-name");
const createLinuxPulseSource = hasArg("--create-linux-pulse-source");
const cleanupFromPath = argValue("--cleanup-from");
const inputFromPath = argValue("--input-from");
const playFixturePath = argValue("--play-fixture");
mkdirSync(artifactDir, { recursive: true });

if (playFixturePath) {
  const playback = await runVirtualAudioPlayback(
    resolve(inputFromPath ?? join(artifactDir, "virtual-audio-input.json")),
    resolve(playFixturePath),
  );
  const playbackPath = join(artifactDir, "virtual-audio-playback.json");
  writeFileSync(playbackPath, `${JSON.stringify(playback, null, 2)}\n`);
  console.log(`Wrote ${playbackPath}`);

  if (playback.failures.length > 0) {
    console.error(playback.failures.join("\n"));
    process.exit(1);
  }
  process.exit(0);
}

if (cleanupFromPath) {
  const cleanup = await runVirtualAudioCleanup(resolve(cleanupFromPath));
  const cleanupPath = join(artifactDir, "virtual-audio-cleanup.json");
  writeFileSync(cleanupPath, `${JSON.stringify(cleanup, null, 2)}\n`);
  console.log(`Wrote ${cleanupPath}`);

  if (cleanup.failures.length > 0) {
    console.error(cleanup.failures.join("\n"));
    process.exit(1);
  }
  process.exit(0);
}

const result = await runVirtualAudioInputPreflight();
const outputPath = join(artifactDir, "virtual-audio-input.json");
writeFileSync(outputPath, `${JSON.stringify(result, null, 2)}\n`);
console.log(`Wrote ${outputPath}`);

if (result.failures.length > 0) {
  console.error(result.failures.join("\n"));
  process.exit(1);
}

async function runVirtualAudioInputPreflight(): Promise<VirtualAudioInputResult> {
  const result: VirtualAudioInputResult = {
    platform: platform(),
    checked: true,
    required: requireVirtualInput,
    created: false,
    sink_name: null,
    source_name: null,
    source_description: null,
    playback_device: null,
    module_ids: [],
    smoke_microphone_arg: null,
    cleanup_commands: [],
    skipped_reason: null,
    failures: [],
  };

  if (requestedDeviceName?.trim()) {
    result.sink_name = null;
    result.source_name = requestedDeviceName.trim();
    result.source_description = requestedDeviceName.trim();
    result.playback_device = null;
    result.smoke_microphone_arg = requestedDeviceName.trim();
    return result;
  }

  if (platform() !== "linux") {
    result.skipped_reason =
      "No built-in virtual input setup is available; install a virtual audio device and pass --device-name";
    if (requireVirtualInput) {
      result.failures.push(result.skipped_reason);
    }
    return result;
  }

  if (!createLinuxPulseSource) {
    result.skipped_reason =
      "Linux virtual input creation requires --create-linux-pulse-source";
    if (requireVirtualInput) {
      result.failures.push(result.skipped_reason);
    }
    return result;
  }

  if (!(await commandExists("pactl"))) {
    result.skipped_reason =
      "pactl is unavailable; install PulseAudio or PipeWire PulseAudio tools";
    if (requireVirtualInput) {
      result.failures.push(result.skipped_reason);
    }
    return result;
  }

  try {
    const sinkModule = await loadPulseModule([
      "module-null-sink",
      "sink_name=verbatim_smoke_sink",
      "sink_properties=device.description=VerbatimSmokeSink",
    ]);
    const sourceModule = await loadPulseModule([
      "module-remap-source",
      "source_name=verbatim_smoke_source",
      "master=verbatim_smoke_sink.monitor",
      "source_properties=device.description=VerbatimSmokeInput",
    ]);

    result.module_ids.push(sinkModule, sourceModule);
    result.cleanup_commands = result.module_ids.map((id) => [
      "pactl",
      "unload-module",
      id,
    ]);

    const sources = await execRequired("pactl", ["list", "short", "sources"]);
    const sourceLine = sources.stdout
      .split(/\r?\n/)
      .find((line) => line.includes("verbatim_smoke_source"));
    if (!sourceLine) {
      result.failures.push("created PulseAudio source was not listed");
      return result;
    }

    result.created = true;
    result.sink_name = "verbatim_smoke_sink";
    result.source_name = "verbatim_smoke_source";
    result.source_description = "VerbatimSmokeInput";
    result.playback_device = "verbatim_smoke_sink";
    result.smoke_microphone_arg = "verbatim_smoke_source";
    return result;
  } catch (error) {
    result.failures.push(
      `failed to create Linux virtual input: ${String(error)}`,
    );
    return result;
  }
}

async function loadPulseModule(moduleArgs: string[]): Promise<string> {
  const loaded = await execRequired("pactl", ["load-module", ...moduleArgs]);
  const moduleId = loaded.stdout.trim();
  if (!/^\d+$/.test(moduleId)) {
    throw new Error(`unexpected pactl module id: ${moduleId}`);
  }
  return moduleId;
}

async function runVirtualAudioCleanup(
  inputPath: string,
): Promise<VirtualAudioCleanupResult> {
  const result: VirtualAudioCleanupResult = {
    checked: true,
    cleanup_from: inputPath,
    cleanup_attempted: false,
    cleanup_commands: [],
    unloaded_module_ids: [],
    skipped_reason: null,
    failures: [],
  };

  if (!existsSync(inputPath)) {
    result.skipped_reason = "virtual audio input artifact does not exist";
    return result;
  }

  let input: VirtualAudioInputResult;
  try {
    input = JSON.parse(
      readFileSync(inputPath, "utf8"),
    ) as VirtualAudioInputResult;
  } catch (error) {
    result.failures.push(
      `failed to read virtual audio input artifact: ${String(error)}`,
    );
    return result;
  }

  result.cleanup_commands = input.cleanup_commands ?? [];
  if (result.cleanup_commands.length === 0) {
    result.skipped_reason =
      "virtual audio input artifact has no cleanup commands";
    return result;
  }

  result.cleanup_attempted = true;
  for (const command of result.cleanup_commands) {
    const [binary, subcommand, moduleId] = command;
    if (
      binary !== "pactl" ||
      subcommand !== "unload-module" ||
      !/^\d+$/.test(moduleId ?? "")
    ) {
      result.failures.push(`unsupported cleanup command: ${command.join(" ")}`);
      continue;
    }

    try {
      await execRequired(binary, [subcommand, moduleId]);
      result.unloaded_module_ids.push(moduleId);
    } catch (error) {
      result.failures.push(
        `failed to unload PulseAudio module ${moduleId}: ${String(error)}`,
      );
    }
  }

  return result;
}

async function runVirtualAudioPlayback(
  inputPath: string,
  fixturePath: string,
): Promise<VirtualAudioPlaybackResult> {
  const result: VirtualAudioPlaybackResult = {
    checked: true,
    input_from: inputPath,
    fixture_path: fixturePath,
    playback_attempted: false,
    playback_device: null,
    playback_command: null,
    skipped_reason: null,
    failures: [],
  };

  if (!existsSync(inputPath)) {
    result.failures.push("virtual audio input artifact does not exist");
    return result;
  }
  if (!existsSync(fixturePath)) {
    result.failures.push("WAV fixture does not exist");
    return result;
  }
  if (platform() !== "linux") {
    result.skipped_reason =
      "fixture playback is currently implemented for Linux PulseAudio/PipeWire only";
    result.failures.push(result.skipped_reason);
    return result;
  }
  if (!(await commandExists("paplay"))) {
    result.failures.push("paplay is unavailable; install pulseaudio-utils");
    return result;
  }

  let input: VirtualAudioInputResult;
  try {
    input = JSON.parse(
      readFileSync(inputPath, "utf8"),
    ) as VirtualAudioInputResult;
  } catch (error) {
    result.failures.push(
      `failed to read virtual audio input artifact: ${String(error)}`,
    );
    return result;
  }

  const playbackDevice = input.playback_device ?? input.sink_name;
  if (!playbackDevice) {
    result.failures.push(
      "virtual audio input artifact does not include a playback device",
    );
    return result;
  }

  result.playback_device = playbackDevice;
  result.playback_command = ["paplay", "--device", playbackDevice, fixturePath];
  result.playback_attempted = true;

  try {
    await execRequired("paplay", ["--device", playbackDevice, fixturePath]);
  } catch (error) {
    result.failures.push(`failed to play WAV fixture: ${String(error)}`);
  }

  return result;
}

async function commandExists(command: string): Promise<boolean> {
  if (platform() === "win32") {
    return execOk("where.exe", [command]);
  }
  return execOk("sh", ["-lc", `command -v ${shellQuote(command)}`]);
}

function shellQuote(value: string): string {
  return `'${value.replaceAll("'", "'\\''")}'`;
}

async function execOk(
  command: string,
  commandArgs: string[],
): Promise<boolean> {
  try {
    await execRequired(command, commandArgs);
    return true;
  } catch {
    return false;
  }
}

function execRequired(
  command: string,
  commandArgs: string[],
): Promise<CommandResult> {
  return new Promise((resolveCommand, rejectCommand) => {
    if (!existsSync(process.cwd())) {
      rejectCommand(new Error("current working directory is unavailable"));
      return;
    }

    execFile(command, commandArgs, (error, stdout, stderr) => {
      if (error) {
        rejectCommand(
          new Error(
            `${command} ${commandArgs.join(" ")} failed: ${stderr || error.message}`,
          ),
        );
        return;
      }
      resolveCommand({ stdout, stderr });
    });
  });
}
