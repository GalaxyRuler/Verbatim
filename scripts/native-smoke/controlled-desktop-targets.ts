import { execFile, spawn } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir, platform } from "node:os";
import { join, resolve } from "node:path";

type CommandResult = {
  stdout: string;
  stderr: string;
};

type DesktopTargetResult = {
  platform: NodeJS.Platform;
  checked: boolean;
  required: boolean;
  target: string | null;
  target_launched: boolean;
  target_focused: boolean;
  text_entry_checked: boolean;
  text_entry_verified: boolean | null;
  text_entry_marker: string | null;
  clipboard_mutation_checked: boolean;
  clipboard_mutation_preserved_marker: boolean | null;
  skipped_reason: string | null;
  failures: string[];
};

const args = process.argv.slice(2);

if (args.includes("--help") || args.includes("-h")) {
  console.log(`Usage: bun scripts/native-smoke/controlled-desktop-targets.ts [options]

Options:
  --artifact-dir <path>        Directory for controlled-desktop-targets.json.
  --allow-text-entry           Type a synthetic marker into the disposable target.
  --allow-clipboard-write      Allow writing a synthetic marker to the OS clipboard.
  --require                    Exit non-zero if a controlled desktop target is unavailable.
  --help                       Show this help text.

The text-entry and clipboard drills use synthetic markers only. The clipboard
drill never reads user clipboard contents. Use mutation options only in isolated
CI or disposable desktop sessions.
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
const requireTarget = hasArg("--require");
const allowTextEntry = hasArg("--allow-text-entry");
const allowClipboardWrite = hasArg("--allow-clipboard-write");
mkdirSync(artifactDir, { recursive: true });

let result: DesktopTargetResult;
try {
  result = await runControlledDesktopTargetDrill({
    allowClipboardWrite,
    allowTextEntry,
    requireTarget,
  });
} catch (error) {
  result = {
    platform: platform(),
    checked: true,
    required: requireTarget,
    target: null,
    target_launched: false,
    target_focused: false,
    text_entry_checked: allowTextEntry,
    text_entry_verified: null,
    text_entry_marker: null,
    clipboard_mutation_checked: allowClipboardWrite,
    clipboard_mutation_preserved_marker: null,
    skipped_reason: null,
    failures: [`controlled desktop target drill crashed: ${String(error)}`],
  };
}
const outputPath = join(artifactDir, "controlled-desktop-targets.json");
writeFileSync(outputPath, `${JSON.stringify(result, null, 2)}\n`);
console.log(`Wrote ${outputPath}`);

if (result.failures.length > 0) {
  console.error(result.failures.join("\n"));
  process.exit(1);
}

async function runControlledDesktopTargetDrill(options: {
  allowClipboardWrite: boolean;
  allowTextEntry: boolean;
  requireTarget: boolean;
}): Promise<DesktopTargetResult> {
  const result: DesktopTargetResult = {
    platform: platform(),
    checked: true,
    required: options.requireTarget,
    target: null,
    target_launched: false,
    target_focused: false,
    text_entry_checked: false,
    text_entry_verified: null,
    text_entry_marker: null,
    clipboard_mutation_checked: false,
    clipboard_mutation_preserved_marker: null,
    skipped_reason: null,
    failures: [],
  };

  const tempDir = await mkdtemp(join(tmpdir(), "verbatim-smoke-target-"));
  const tempFile = join(tempDir, "target.txt");
  await writeFile(tempFile, "verbatim controlled paste target\n", "utf8");

  try {
    const target = await launchTarget(tempFile);
    if (!target) {
      result.skipped_reason = targetSkipReason();
      if (options.requireTarget) {
        result.failures.push(result.skipped_reason);
      }
      return result;
    }

    result.target = target.name;
    result.target_launched = true;
    result.target_focused = await target.focus();

    if (!result.target_focused && options.requireTarget) {
      result.failures.push(`failed to focus ${target.name}`);
    }

    try {
      if (options.allowTextEntry) {
        result.text_entry_checked = true;
        result.text_entry_marker = `verbatimatarget${Date.now()}`;
        result.text_entry_verified = await target.typeMarker(
          result.text_entry_marker,
          tempFile,
        );
        if (!result.text_entry_verified) {
          result.failures.push(
            "synthetic text-entry marker was not saved by the target",
          );
        }
      }

      if (options.allowClipboardWrite) {
        result.clipboard_mutation_checked = true;
        result.clipboard_mutation_preserved_marker =
          await verifySyntheticClipboardMutation(target.name);
        if (!result.clipboard_mutation_preserved_marker) {
          result.failures.push(
            "synthetic clipboard mutation marker was not readable",
          );
        }
      } else {
        result.skipped_reason =
          "clipboard mutation drill skipped without --allow-clipboard-write";
      }
    } catch (error) {
      result.failures.push(
        `synthetic clipboard mutation drill failed: ${String(error)}`,
      );
    } finally {
      await target.close();
    }

    return result;
  } finally {
    await rm(tempDir, { recursive: true, force: true });
  }
}

type ControlledTarget = {
  name: string;
  focus: () => Promise<boolean>;
  typeMarker: (marker: string, filePath: string) => Promise<boolean>;
  close: () => Promise<void>;
};

async function launchTarget(
  filePath: string,
): Promise<ControlledTarget | null> {
  switch (platform()) {
    case "win32":
      return launchWindowsNotepad(filePath);
    case "darwin":
      return launchMacTextEdit(filePath);
    case "linux":
      return launchLinuxGtkTarget(filePath);
    default:
      return null;
  }
}

function targetSkipReason(): string {
  switch (platform()) {
    case "win32":
      return "Notepad is unavailable";
    case "darwin":
      return "TextEdit automation is unavailable";
    case "linux":
      return "No supported Linux text target found; install gedit, mousepad, or xterm in the smoke desktop session";
    default:
      return `Unsupported platform: ${platform()}`;
  }
}

async function launchWindowsNotepad(
  filePath: string,
): Promise<ControlledTarget | null> {
  const child = spawn("notepad.exe", [filePath], {
    detached: false,
    stdio: "ignore",
    windowsHide: false,
  });
  await delay(750);
  if (child.exitCode !== null) return null;

  return {
    name: "notepad",
    focus: async () => {
      const script = [
        "$shell = New-Object -ComObject WScript.Shell",
        "$shell.AppActivate('target.txt - Notepad') -or $shell.AppActivate('Notepad')",
      ].join("; ");
      const focused = await execOk("powershell.exe", [
        "-NoProfile",
        "-Command",
        script,
      ]);
      await delay(250);
      return focused;
    },
    typeMarker: async (marker, targetPath) => {
      const script = [
        "$shell = New-Object -ComObject WScript.Shell",
        "$null = $shell.AppActivate('target.txt - Notepad') -or $shell.AppActivate('Notepad')",
        `Start-Sleep -Milliseconds 250`,
        `$shell.SendKeys(${JSON.stringify(marker)})`,
        `Start-Sleep -Milliseconds 250`,
        "$shell.SendKeys('^s')",
      ].join("; ");
      if (
        !(await execOk("powershell.exe", ["-NoProfile", "-Command", script]))
      ) {
        return false;
      }
      await delay(750);
      return fileContains(targetPath, marker);
    },
    close: async () => {
      child.kill();
      await delay(250);
    },
  };
}

async function launchMacTextEdit(
  filePath: string,
): Promise<ControlledTarget | null> {
  if (!(await execOk("open", ["-a", "TextEdit", filePath]))) return null;
  await delay(1000);
  return {
    name: "textedit",
    focus: async () =>
      execOk("osascript", ["-e", 'tell application "TextEdit" to activate']),
    typeMarker: async (marker, targetPath) => {
      const script = [
        'tell application "TextEdit" to activate',
        'tell application "System Events"',
        `keystroke ${JSON.stringify(marker)}`,
        'keystroke "s" using command down',
        "end tell",
      ].join("\n");
      if (!(await execOk("osascript", ["-e", script]))) {
        return false;
      }
      await delay(750);
      return fileContains(targetPath, marker);
    },
    close: async () => {
      await execOk("osascript", [
        "-e",
        'tell application "TextEdit" to close every document saving no',
      ]);
    },
  };
}

async function launchLinuxGtkTarget(
  filePath: string,
): Promise<ControlledTarget | null> {
  const candidates = [
    { command: "gedit", args: [filePath], name: "gedit", title: "gedit" },
    {
      command: "mousepad",
      args: [filePath],
      name: "mousepad",
      title: "mousepad",
    },
    {
      command: "xterm",
      args: [
        "-T",
        "verbatim-smoke-xterm",
        "-e",
        "sh",
        "-c",
        `cat > ${shellQuote(filePath)}; sleep 2`,
      ],
      name: "xterm",
      title: "verbatim-smoke-xterm",
    },
  ];

  for (const candidate of candidates) {
    if (!(await commandExists(candidate.command))) continue;
    const child = spawn(candidate.command, candidate.args, {
      detached: false,
      stdio: "ignore",
    });
    await delay(1000);
    if (child.exitCode !== null) continue;
    return {
      name: candidate.name,
      focus: async () => {
        if (!(await commandExists("xdotool"))) return false;
        return execOk("xdotool", [
          "search",
          "--name",
          candidate.title,
          "windowactivate",
        ]);
      },
      typeMarker: async (marker, targetPath) => {
        if (!(await commandExists("xdotool"))) return false;
        const focused = await execOk("xdotool", [
          "search",
          "--name",
          candidate.title,
          "windowactivate",
        ]);
        if (!focused) return false;
        await delay(250);
        if (!(await execOk("xdotool", ["type", "--delay", "1", marker]))) {
          return false;
        }
        if (candidate.name === "xterm") {
          await execOk("xdotool", ["key", "Return"]);
          await execOk("xdotool", ["key", "ctrl+d"]);
        } else {
          await execOk("xdotool", ["key", "ctrl+s"]);
        }
        await delay(750);
        return fileContains(targetPath, marker);
      },
      close: async () => {
        child.kill();
        await delay(250);
      },
    };
  }

  return null;
}

async function verifySyntheticClipboardMutation(
  targetName: string,
): Promise<boolean> {
  const marker = `verbatim-smoke-clipboard-${targetName}-${Date.now()}`;
  switch (platform()) {
    case "win32":
      await execRequired("powershell.exe", [
        "-NoProfile",
        "-Command",
        `Set-Clipboard -Value ${JSON.stringify(marker)}`,
      ]);
      return (
        (
          await execRequired("powershell.exe", [
            "-NoProfile",
            "-Command",
            "Get-Clipboard -Raw",
          ])
        ).stdout.trim() === marker
      );
    case "darwin":
      await execRequired("pbcopy", [], marker);
      return (await execRequired("pbpaste", [])).stdout.trim() === marker;
    case "linux":
      if (await commandExists("wl-copy")) {
        await execRequired("wl-copy", [], marker);
        return (await execRequired("wl-paste", [])).stdout.trim() === marker;
      }
      if (await commandExists("xclip")) {
        await execRequired("xclip", ["-selection", "clipboard"], marker);
        return (
          (
            await execRequired("xclip", ["-selection", "clipboard", "-o"])
          ).stdout.trim() === marker
        );
      }
      if (await commandExists("xsel")) {
        await execRequired("xsel", ["--clipboard", "--input"], marker);
        return (
          (
            await execRequired("xsel", ["--clipboard", "--output"])
          ).stdout.trim() === marker
        );
      }
      throw new Error("No supported Linux clipboard helper found");
    default:
      return false;
  }
}

async function fileContains(path: string, marker: string): Promise<boolean> {
  try {
    return (await readFile(path, "utf8")).includes(marker);
  } catch {
    return false;
  }
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
  args: string[],
  input?: string,
  shell?: string,
): Promise<boolean> {
  try {
    await execRequired(command, args, input, shell);
    return true;
  } catch {
    return false;
  }
}

function execRequired(
  command: string,
  args: string[],
  input?: string,
  shell?: string,
): Promise<CommandResult> {
  return new Promise((resolveCommand, rejectCommand) => {
    const child = execFile(
      command,
      args,
      { shell },
      (error, stdout, stderr) => {
        if (error) {
          rejectCommand(
            new Error(
              `${command} ${args.join(" ")} failed: ${stderr || error.message}`,
            ),
          );
          return;
        }
        resolveCommand({ stdout, stderr });
      },
    );
    if (input !== undefined) {
      child.stdin?.end(input);
    }
  });
}

function delay(ms: number): Promise<void> {
  return new Promise((resolveDelay) => setTimeout(resolveDelay, ms));
}
