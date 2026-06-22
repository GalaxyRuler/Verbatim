// Phase 1 / T-CUTOVER adapter. Prefers the typed verbatim-android Tauri plugin, falling back
// to the legacy window.VerbatimAndroid @JavascriptInterface bridge until the migration completes.
import {
  addPluginListener,
  invoke,
  type PluginListener,
} from "@tauri-apps/api/core";

const PLUGIN = "verbatim-android";

/** Pull the current permission snapshot from the native plugin (falls back to the legacy bridge). */
export async function permissionSnapshot(): Promise<Record<string, unknown>> {
  try {
    return (
      (await invoke<Record<string, unknown>>(
        `plugin:${PLUGIN}|permission_snapshot`,
      )) ?? {}
    );
  } catch {
    const raw = window.VerbatimAndroid?.permissionSnapshot?.();
    if (!raw) return {};
    try {
      return JSON.parse(raw) as Record<string, unknown>;
    } catch {
      return {};
    }
  }
}

/** Subscribe to native permission/state pushes (replaces the 1.2s polling, ADR-1). */
export function onPermissions(
  cb: (snapshot: Record<string, unknown>) => void,
): Promise<PluginListener> {
  return addPluginListener(
    PLUGIN,
    "permissions",
    cb as (event: unknown) => void,
  );
}
