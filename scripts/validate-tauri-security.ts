import { existsSync, readdirSync, readFileSync } from "node:fs";
import path from "node:path";

type CapabilityPermission = string | { identifier?: string };

type CapabilityFile = {
  identifier?: string;
  windows?: string[];
  permissions?: CapabilityPermission[];
};

type TauriConfig = {
  app?: {
    security?: {
      csp?: unknown;
      assetProtocol?: {
        scope?: {
          allow?: unknown[];
        };
      };
    };
  };
  bundle?: {
    macOS?: {
      hardenedRuntime?: unknown;
      minimumSystemVersion?: unknown;
      entitlements?: unknown;
    };
  };
};

const repoRoot = process.cwd();
const capabilityDir = path.join(repoRoot, "src-tauri", "capabilities");
const tauriConfigPath = path.join(repoRoot, "src-tauri", "tauri.conf.json");

const failures: string[] = [];

function readJson<T>(filePath: string): T {
  return JSON.parse(readFileSync(filePath, "utf8")) as T;
}

function permissionId(permission: CapabilityPermission): string {
  if (typeof permission === "string") return permission;
  return permission.identifier ?? JSON.stringify(permission);
}

function stringValue(value: unknown): string {
  if (typeof value === "string") return value;
  if (value && typeof value === "object" && "path" in value) {
    return String((value as { path?: unknown }).path ?? "");
  }
  return JSON.stringify(value);
}

const tauriConfig = readJson<TauriConfig>(tauriConfigPath);
const security = tauriConfig.app?.security;
const macOSBundle = tauriConfig.bundle?.macOS;

if (security?.csp == null) {
  failures.push(
    "src-tauri/tauri.conf.json must not set app.security.csp to null.",
  );
}

function cspDirective(name: string): string {
  const csp = security?.csp;
  if (typeof csp === "string") {
    const directive = csp
      .split(";")
      .map((part) => part.trim())
      .find((part) => part.startsWith(`${name} `));
    return directive ?? "";
  }
  if (csp && typeof csp === "object" && name in csp) {
    return String((csp as Record<string, unknown>)[name] ?? "");
  }
  return "";
}

function requireCspSource(directive: string, source: string, reason: string) {
  const value = cspDirective(directive);
  const sources = new Set(value.split(/\s+/).filter(Boolean));
  if (!sources.has(source)) {
    failures.push(
      `CSP directive '${directive}' must include '${source}' for ${reason}.`,
    );
  }
}

requireCspSource("script-src", "'self'", "Vite dynamic locale chunk loading");
for (const directive of ["img-src", "media-src"]) {
  requireCspSource(directive, "asset:", "Tauri asset protocol URLs");
  requireCspSource(
    directive,
    "http://asset.localhost",
    "Tauri asset protocol URLs on platforms using the localhost asset origin",
  );
}

const assetScope = security?.assetProtocol?.scope?.allow ?? [];
for (const scopeEntry of assetScope) {
  const scope = stringValue(scopeEntry);
  if (scope === "**" || scope.includes("/**") || scope.includes("\\**")) {
    failures.push(
      `assetProtocol scope must not contain broad wildcard '${scope}'.`,
    );
  }
}

function escapeRegex(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function plistHasTrueEntitlement(plist: string, entitlement: string): boolean {
  return new RegExp(
    `<key>\\s*${escapeRegex(entitlement)}\\s*</key>\\s*<true\\s*/>`,
  ).test(plist);
}

if (macOSBundle?.hardenedRuntime !== true) {
  failures.push(
    "src-tauri/tauri.conf.json bundle.macOS.hardenedRuntime must be true.",
  );
}

if (
  typeof macOSBundle?.minimumSystemVersion !== "string" ||
  macOSBundle.minimumSystemVersion.trim().length === 0
) {
  failures.push(
    "src-tauri/tauri.conf.json bundle.macOS.minimumSystemVersion must be set.",
  );
}

if (
  typeof macOSBundle?.entitlements !== "string" ||
  macOSBundle.entitlements.trim().length === 0
) {
  failures.push(
    "src-tauri/tauri.conf.json bundle.macOS.entitlements must point to the reviewed entitlements plist.",
  );
} else {
  const entitlementsPath = path.join(
    repoRoot,
    "src-tauri",
    macOSBundle.entitlements,
  );

  if (!existsSync(entitlementsPath)) {
    failures.push(
      `macOS entitlements file '${macOSBundle.entitlements}' does not exist.`,
    );
  } else {
    const entitlements = readFileSync(entitlementsPath, "utf8");
    const requiredEntitlements = [
      "com.apple.security.device.microphone",
      "com.apple.security.device.audio-input",
    ];
    const forbiddenEntitlements = [
      "com.apple.security.cs.allow-dyld-environment-variables",
      "com.apple.security.cs.disable-library-validation",
      "com.apple.security.get-task-allow",
    ];

    for (const entitlement of requiredEntitlements) {
      if (!plistHasTrueEntitlement(entitlements, entitlement)) {
        failures.push(
          `macOS entitlements must enable '${entitlement}' for recording support.`,
        );
      }
    }

    for (const entitlement of forbiddenEntitlements) {
      if (plistHasTrueEntitlement(entitlements, entitlement)) {
        failures.push(
          `macOS entitlements must not enable '${entitlement}' in release configuration.`,
        );
      }
    }
  }
}

const capabilityFiles = readdirSync(capabilityDir)
  .filter((fileName) => fileName.endsWith(".json"))
  .sort();

let overlayCapabilityCount = 0;

for (const fileName of capabilityFiles) {
  const capabilityPath = path.join(capabilityDir, fileName);
  const capability = readJson<CapabilityFile>(capabilityPath);
  const permissions = capability.permissions ?? [];
  const seenPermissions = new Set<string>();

  for (const permission of permissions) {
    const id = permissionId(permission);
    if (seenPermissions.has(id)) {
      failures.push(`${fileName} contains duplicate permission '${id}'.`);
    }
    seenPermissions.add(id);

    if (
      id === "fs:scope" &&
      typeof permission === "object" &&
      "allow" in permission
    ) {
      const allowEntries = (permission as { allow?: unknown[] }).allow ?? [];
      for (const allowEntry of allowEntries) {
        const allowPath = stringValue(allowEntry);
        if (
          allowPath === "$APPDATA" ||
          allowPath === "$APPDATA/**/*" ||
          allowPath === "$APPDATA/**"
        ) {
          failures.push(
            `${fileName} must not grant broad app-data fs scope '${allowPath}'.`,
          );
        }
      }
    }
  }

  const windows = capability.windows ?? [];
  if (!windows.includes("recording_overlay")) continue;

  overlayCapabilityCount += 1;

  for (const permission of permissions) {
    const id = permissionId(permission);
    if (!id.startsWith("core:")) {
      failures.push(
        `${fileName} grants '${id}' to recording_overlay; overlay capabilities must remain core-only.`,
      );
    }
  }
}

if (overlayCapabilityCount !== 1) {
  failures.push(
    `recording_overlay must be assigned to exactly one capability file; found ${overlayCapabilityCount}.`,
  );
}

if (failures.length > 0) {
  console.error("Tauri security validation failed:");
  for (const failure of failures) {
    console.error(`- ${failure}`);
  }
  process.exit(1);
}

console.log("Tauri security validation passed.");
