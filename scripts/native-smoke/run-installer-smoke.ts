import { spawn } from "node:child_process";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { homedir, tmpdir, platform } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { findFiles, findMountedMacApp } from "./installer-smoke-utils.js";

type CommandResult = {
  stdout: string;
  stderr: string;
};

type CommandOptions = {
  env?: NodeJS.ProcessEnv;
};

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
const tauriRoot = join(repoRoot, "src-tauri");
const tauriConfigPath = join(tauriRoot, "tauri.conf.json");
const hostPlatform = platform();
const args = process.argv.slice(2);

if (args.includes("--help") || args.includes("-h")) {
  console.log(`Usage: bun scripts/native-smoke/run-installer-smoke.ts [options]

Options:
  --artifact-dir <path>  Directory for installer logs and delegated smoke artifacts.
  --timeout-ms <number>  Per-launch timeout passed to the packaged smoke runner.
  --help                Show this help text.
`);
  process.exit(0);
}

const artifactDir = resolve(
  argValue("--artifact-dir") ??
    process.env.VERBATIM_INSTALLER_SMOKE_ARTIFACT_DIR ??
    join(repoRoot, "native-smoke-artifacts", "installer"),
);
const timeoutMs = argValue("--timeout-ms") ?? "30000";
const tempRoot = mkdtempSync(join(tmpdir(), "verbatim-installer-smoke-"));
let installedAppPath: string | null = null;
let linuxPackageName: string | null = null;
let linuxInstalledAppPath: string | null = null;
let windowsInstallDir: string | null = null;
let windowsUninstallerPath: string | null = null;
let windowsAppDataProbe: {
  roamingRoot: string;
  localRoot: string;
  roamingAppDir: string;
  localAppDir: string;
  markerFiles: string[];
} | null = null;

mkdirSync(artifactDir, { recursive: true });

const summary: Record<string, unknown> = {
  platform: hostPlatform,
  artifactDir,
  tempRoot,
};

try {
  installedAppPath =
    hostPlatform === "win32"
      ? await installWindowsNormal()
      : hostPlatform === "darwin"
        ? await installMacDmg()
        : await installLinuxDeb();

  summary.installedAppPath = installedAppPath;
  writeSummary();
  await runPackagedSmoke(installedAppPath);
  console.log(
    `Installer smoke completed for ${basename(installedAppPath)}. Artifacts: ${artifactDir}`,
  );
} finally {
  if (hostPlatform === "win32" && windowsUninstallerPath) {
    await runWindowsUninstallCycle({
      appPath: installedAppPath,
      deleteAppData: false,
      logName: "windows-uninstaller-preserve-app-data",
    }).catch((error) => {
      writeFileSync(
        join(artifactDir, "windows-uninstaller-preserve-app-data.error.log"),
        String(error),
      );
      process.exitCode = 1;
    });

    if (process.exitCode == null) {
      await installWindowsNormal("delete-app-data")
        .then((deleteAppPath) =>
          runWindowsUninstallCycle({
            appPath: deleteAppPath,
            deleteAppData: true,
            logName: "windows-uninstaller-delete-app-data",
          }),
        )
        .catch((error) => {
          writeFileSync(
            join(artifactDir, "windows-uninstaller-delete-app-data.error.log"),
            String(error),
          );
          process.exitCode = 1;
        });
    }
  }

  if (hostPlatform === "linux" && linuxPackageName) {
    await runCommand(
      "sudo",
      ["apt-get", "remove", "-y", linuxPackageName],
      "linux-apt-remove",
      120_000,
    ).catch((error) => {
      writeFileSync(
        join(artifactDir, "linux-apt-remove.error.log"),
        String(error),
      );
      process.exitCode = 1;
    });

    if (
      linuxInstalledAppPath &&
      !(await waitForPathMissing(linuxInstalledAppPath, 10_000))
    ) {
      writeFileSync(
        join(artifactDir, "linux-uninstall-verification.error.log"),
        `Installed app still exists after package removal: ${linuxInstalledAppPath}`,
      );
      process.exitCode = 1;
    }
  }
  rmSync(tempRoot, { recursive: true, force: true });
}

function argValue(name: string): string | undefined {
  const index = args.indexOf(name);
  if (index >= 0) return args[index + 1];
  const prefix = `${name}=`;
  return args.find((arg) => arg.startsWith(prefix))?.slice(prefix.length);
}

async function installWindowsNormal(
  cycle = "preserve-app-data",
): Promise<string> {
  assertWindowsAppDataDeleteOptionTemplate();
  const installer = findNewestBundleFile(["release", "bundle", "nsis"], ".exe");
  const installDir = join(tempRoot, `Verbatim-${cycle}`);
  mkdirSync(installDir, { recursive: true });
  windowsInstallDir = installDir;
  summary.installer = installer;
  summary.installDir = installDir;
  summary.windowsInstallCycle = cycle;
  writeSummary();

  await runCommand(
    installer,
    ["/S", "/NS", `/D=${installDir}`],
    `windows-installer-${cycle}`,
    120_000,
  );

  const appPath = join(installDir, "Verbatim.exe");
  if (!existsSync(appPath)) {
    throw new Error(`Installed Windows app was not found: ${appPath}`);
  }
  windowsUninstallerPath = join(installDir, "uninstall.exe");
  if (!existsSync(windowsUninstallerPath)) {
    throw new Error(
      `Installed Windows uninstaller was not found: ${windowsUninstallerPath}`,
    );
  }
  return appPath;
}

async function installMacDmg(): Promise<string> {
  const dmg = findNewestBundleFile(["release", "bundle", "dmg"], ".dmg");
  const mountDir = join(tempRoot, "dmg");
  const installDir = join(tempRoot, "Applications");
  mkdirSync(mountDir, { recursive: true });
  mkdirSync(installDir, { recursive: true });
  summary.installer = dmg;
  summary.mountDir = mountDir;
  summary.installDir = installDir;
  writeSummary();

  await runCommand(
    "hdiutil",
    ["attach", dmg, "-nobrowse", "-readonly", "-mountpoint", mountDir],
    "macos-hdiutil-attach",
    120_000,
  );

  try {
    const mountedApp = findMountedMacApp(mountDir);
    const installedAppBundle = join(installDir, basename(mountedApp));
    await runCommand(
      "ditto",
      [mountedApp, installedAppBundle],
      "macos-copy-app",
      120_000,
    );
    await runCommand(
      "xattr",
      ["-cr", installedAppBundle],
      "macos-clear-quarantine",
      60_000,
    );
  } finally {
    await runCommand(
      "hdiutil",
      ["detach", mountDir, "-quiet"],
      "macos-hdiutil-detach",
      60_000,
    );
  }

  const appPath = join(
    installDir,
    "Verbatim.app",
    "Contents",
    "MacOS",
    "Verbatim",
  );
  if (!existsSync(appPath)) {
    throw new Error(`Installed macOS app was not found: ${appPath}`);
  }
  return appPath;
}

async function installLinuxDeb(): Promise<string> {
  const deb = findNewestBundleFile(["release", "bundle", "deb"], ".deb");
  summary.installer = deb;
  writeSummary();

  const packageName = (
    await runCommand(
      "dpkg-deb",
      ["-f", deb, "Package"],
      "linux-package-name",
      30_000,
    )
  ).stdout.trim();
  linuxPackageName = packageName;
  summary.packageName = packageName;
  writeSummary();

  await runCommand(
    "sudo",
    ["apt-get", "install", "-y", "--no-install-recommends", deb],
    "linux-apt-install",
    180_000,
  );

  const appPath = ["/usr/bin/verbatim", "/usr/bin/verbatim-app"].find(
    (candidate) => existsSync(candidate),
  );
  if (!appPath) {
    throw new Error("Installed Linux app was not found in /usr/bin.");
  }
  linuxInstalledAppPath = appPath;

  return appPath;
}

async function runPackagedSmoke(appPath: string): Promise<void> {
  const delegatedArtifactDir = join(artifactDir, "packaged-smoke");
  mkdirSync(delegatedArtifactDir, { recursive: true });
  await runCommand(
    "bun",
    [
      join(repoRoot, "scripts", "native-smoke", "run-packaged-smoke.ts"),
      "--app",
      appPath,
      "--artifact-dir",
      delegatedArtifactDir,
      "--timeout-ms",
      timeoutMs,
    ],
    "packaged-smoke",
    Number(timeoutMs) * 6,
  );
}

function findNewestBundleFile(
  relativeBundlePath: string[],
  extension: string,
): string {
  const targetDir = resolve(
    process.env.CARGO_TARGET_DIR ?? join(tauriRoot, "target"),
  );
  const candidates = [
    join(targetDir, ...relativeBundlePath),
    join(targetDir, "x86_64-pc-windows-msvc", ...relativeBundlePath),
    join(targetDir, "x86_64-unknown-linux-gnu", ...relativeBundlePath),
    join(targetDir, "aarch64-apple-darwin", ...relativeBundlePath),
  ];

  const files = candidates.flatMap((candidate) =>
    existsSync(candidate) ? findFiles(candidate, extension) : [],
  );
  files.sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs);
  const found = files[0];
  if (!found) {
    throw new Error(
      `No ${extension} installer found. Checked: ${candidates.join(", ")}`,
    );
  }
  return found;
}

function findFirstFile(root: string, extension: string): string {
  const found = findFiles(root, extension)[0];
  if (!found) throw new Error(`No ${extension} file found under ${root}`);
  return found;
}

function runCommand(
  command: string,
  commandArgs: string[],
  logName: string,
  maxMs: number,
  options: CommandOptions = {},
): Promise<CommandResult> {
  return new Promise((resolveCommand, rejectCommand) => {
    const child = spawn(command, commandArgs, {
      cwd: repoRoot,
      env: options.env ?? process.env,
      windowsHide: true,
    });
    let stdout = "";
    let stderr = "";
    const timer = setTimeout(() => {
      child.kill("SIGTERM");
      rejectCommand(new Error(`${logName} timed out after ${maxMs}ms`));
    }, maxMs);

    child.stdout?.on("data", (chunk) => {
      stdout += String(chunk);
    });
    child.stderr?.on("data", (chunk) => {
      stderr += String(chunk);
    });
    child.on("error", (error) => {
      clearTimeout(timer);
      rejectCommand(error);
    });
    child.on("exit", (code, signal) => {
      clearTimeout(timer);
      writeFileSync(join(artifactDir, `${logName}.stdout.log`), stdout);
      writeFileSync(join(artifactDir, `${logName}.stderr.log`), stderr);
      if (code === 0) {
        resolveCommand({ stdout, stderr });
      } else {
        rejectCommand(
          new Error(`${logName} exited with code ${code} signal ${signal}`),
        );
      }
    });
  });
}

function readBundleIdentifier(): string {
  const config = JSON.parse(readFileSync(tauriConfigPath, "utf8")) as {
    identifier?: unknown;
  };
  if (typeof config.identifier !== "string" || config.identifier.length === 0) {
    throw new Error("src-tauri/tauri.conf.json identifier must be set.");
  }
  return config.identifier;
}

function assertWindowsAppDataDeleteOptionTemplate(): void {
  const template = readFileSync(
    join(tauriRoot, "nsis", "installer.nsi"),
    "utf8",
  );
  const requiredSnippets = [
    "Var DeleteAppDataCheckbox",
    "Var DeleteAppDataCheckboxState",
    'GetOptions} $CMDLINE "/DELETEAPPDATA"',
    'RmDir /r "$APPDATA\\${BUNDLEID}"',
    'RmDir /r "$LOCALAPPDATA\\${BUNDLEID}"',
  ];
  const missing = requiredSnippets.filter(
    (snippet) => !template.includes(snippet),
  );
  if (missing.length > 0) {
    throw new Error(
      `Windows NSIS app-data deletion option is incomplete: ${missing.join(", ")}`,
    );
  }
}

function prepareWindowsAppDataProbe(
  cycle: string,
): NonNullable<typeof windowsAppDataProbe> {
  const bundleIdentifier = readBundleIdentifier();
  // NSIS resolves $APPDATA/$LOCALAPPDATA from current-user shell folders, not env overrides.
  const roamingRoot = readCurrentWindowsShellFolder("APPDATA", [
    "AppData",
    "Roaming",
  ]);
  const localRoot = readCurrentWindowsShellFolder("LOCALAPPDATA", [
    "AppData",
    "Local",
  ]);
  const roamingAppDir = join(roamingRoot, bundleIdentifier);
  const localAppDir = join(localRoot, bundleIdentifier);
  const markerFiles = [
    join(roamingAppDir, "settings.json"),
    join(roamingAppDir, "history.db"),
    join(roamingAppDir, "recordings", "probe.wav"),
    join(localAppDir, "cache", "probe.bin"),
  ];

  assertWindowsAppDataProbeTargetCanBeSeeded(roamingAppDir);
  assertWindowsAppDataProbeTargetCanBeSeeded(localAppDir);

  for (const markerFile of markerFiles) {
    mkdirSync(dirname(markerFile), { recursive: true });
    writeFileSync(markerFile, "native smoke app-data retention probe\n");
  }

  return {
    roamingRoot,
    localRoot,
    roamingAppDir,
    localAppDir,
    markerFiles,
  };
}

function seedWindowsAppDataProbe(cycle: string): void {
  windowsAppDataProbe = prepareWindowsAppDataProbe(cycle);
  summary.windowsAppDataProbe = {
    cycle,
    seededRoaming: true,
    seededLocal: true,
    expectedDeleteAppData: cycle === "delete-app-data",
  };
  writeSummary();
}

function readCurrentWindowsShellFolder(
  envName: "APPDATA" | "LOCALAPPDATA",
  fallbackParts: string[],
): string {
  const value = process.env[envName];
  if (value && value.trim().length > 0) {
    return value;
  }
  return join(homedir(), ...fallbackParts);
}

function assertWindowsAppDataProbeTargetCanBeSeeded(appDir: string): void {
  if (!existsSync(appDir)) return;
  if (allowsExistingWindowsAppDataProbeTarget()) return;
  throw new Error(
    `Windows installer smoke refuses to run /DELETEAPPDATA against existing app data: ${appDir}. Run on a clean profile or set VERBATIM_INSTALLER_SMOKE_ALLOW_EXISTING_APPDATA=1 only for disposable CI profiles.`,
  );
}

function allowsExistingWindowsAppDataProbeTarget(): boolean {
  return (
    process.env.GITHUB_ACTIONS === "true" ||
    process.env.VERBATIM_INSTALLER_SMOKE_ALLOW_EXISTING_APPDATA === "1"
  );
}

function cleanupWindowsAppDataProbe(): void {
  if (!windowsAppDataProbe) return;
  const cleanedAppDirs = [
    windowsAppDataProbe.roamingAppDir,
    windowsAppDataProbe.localAppDir,
  ];
  for (const appDir of cleanedAppDirs) {
    rmSync(appDir, { recursive: true, force: true });
  }
  summary.windowsAppDataProbeCleanup = {
    cleanedAppDirs,
  };
  windowsAppDataProbe = null;
  writeSummary();
}

async function runWindowsUninstallCycle(options: {
  appPath: string | null;
  deleteAppData: boolean;
  logName: string;
}): Promise<void> {
  if (!windowsUninstallerPath) return;
  const uninstallArgs = options.deleteAppData
    ? ["/S", "/P", "/DELETEAPPDATA"]
    : ["/S", "/P"];

  try {
    seedWindowsAppDataProbe(
      options.deleteAppData ? "delete-app-data" : "preserve-app-data",
    );

    await runCommand(
      windowsUninstallerPath,
      uninstallArgs,
      options.logName,
      120_000,
    );

    if (
      options.appPath &&
      !(await waitForPathMissing(options.appPath, 10_000))
    ) {
      throw new Error(
        `Installed app still exists after uninstall: ${options.appPath}`,
      );
    }

    if (windowsInstallDir && existsSync(windowsInstallDir)) {
      writeFileSync(
        join(artifactDir, `${options.logName}-install-dir-remaining.log`),
        `Install directory remains after uninstall: ${windowsInstallDir}`,
      );
    }

    if (options.deleteAppData) {
      verifyWindowsDeleteAppDataUninstallRemovedAppData();
    } else {
      verifyWindowsSilentUninstallPreservedAppData();
    }
  } finally {
    cleanupWindowsAppDataProbe();
  }
}

function verifyWindowsSilentUninstallPreservedAppData(): void {
  if (!windowsAppDataProbe) return;

  const missingMarkers = windowsAppDataProbe.markerFiles
    .map((markerFile, index) => ({ marker: `marker-${index + 1}`, markerFile }))
    .filter(({ markerFile }) => !existsSync(markerFile))
    .map(({ marker }) => marker);

  const result = {
    silentUninstallPreservedRoamingAppData: existsSync(
      windowsAppDataProbe.roamingAppDir,
    ),
    silentUninstallPreservedLocalAppData: existsSync(
      windowsAppDataProbe.localAppDir,
    ),
    missingMarkers,
  };
  summary.windowsAppDataProbe = {
    ...(summary.windowsAppDataProbe as Record<string, unknown> | undefined),
    ...result,
  };
  writeSummary();

  if (
    !result.silentUninstallPreservedRoamingAppData ||
    !result.silentUninstallPreservedLocalAppData ||
    missingMarkers.length > 0
  ) {
    throw new Error(
      `Windows silent uninstall unexpectedly removed app-data probe markers: ${missingMarkers.join(", ")}`,
    );
  }
}

function verifyWindowsDeleteAppDataUninstallRemovedAppData(): void {
  if (!windowsAppDataProbe) return;

  const remainingMarkers = windowsAppDataProbe.markerFiles
    .map((markerFile, index) => ({ marker: `marker-${index + 1}`, markerFile }))
    .filter(({ markerFile }) => existsSync(markerFile))
    .map(({ marker }) => marker);

  const result = {
    deleteAppDataRemovedRoamingAppData: !existsSync(
      windowsAppDataProbe.roamingAppDir,
    ),
    deleteAppDataRemovedLocalAppData: !existsSync(
      windowsAppDataProbe.localAppDir,
    ),
    remainingMarkers,
  };
  summary.windowsAppDataProbe = {
    ...(summary.windowsAppDataProbe as Record<string, unknown> | undefined),
    ...result,
  };
  writeSummary();

  if (
    !result.deleteAppDataRemovedRoamingAppData ||
    !result.deleteAppDataRemovedLocalAppData ||
    remainingMarkers.length > 0
  ) {
    throw new Error(
      `Windows /DELETEAPPDATA uninstall left app-data probe markers: ${remainingMarkers.join(", ")}`,
    );
  }
}

async function waitForPathMissing(
  filePath: string,
  maxMs: number,
): Promise<boolean> {
  const started = Date.now();
  while (Date.now() - started < maxMs) {
    if (!existsSync(filePath)) return true;
    await new Promise((resolveWait) => setTimeout(resolveWait, 250));
  }
  return !existsSync(filePath);
}

function writeSummary(): void {
  writeFileSync(
    join(artifactDir, "installer-smoke-summary.json"),
    JSON.stringify(summary, null, 2),
  );
}
