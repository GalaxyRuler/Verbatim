// Phase 1 / T-CUTOVER adapter. Prefers the typed verbatim-android Tauri plugin, falling back
// to the legacy window.VerbatimAndroid @JavascriptInterface bridge until the migration completes
// (the raw bridge stays as a rollback path until the plugin survives a staged release).
import {
  addPluginListener,
  invoke,
  type PluginListener,
} from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

const PLUGIN = "verbatim-android";

const raw = () => window.VerbatimAndroid;

function cmd<T>(name: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(`plugin:${PLUGIN}|${name}`, args);
}

function appCmd<T>(name: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(name, args);
}

export type AndroidAsrModelPackState = {
  id: string;
  displayName: string;
  description: string;
  language: string;
  sizeMb: number;
  installedDir: string;
  isInstalled: boolean;
  isDownloading: boolean;
  isActive: boolean;
  isSelectable: boolean;
  downloadPhase: string;
  downloadProgress: number;
  missingFiles: string[];
};

export type AndroidAsrDownloadProgress = {
  modelId: string;
  phase: string;
  file?: string | null;
  downloaded: number;
  total: number;
  percentage: number;
};

// ---- State (pull + push) ----

/** Pull the current permission snapshot from the native plugin (falls back to the legacy bridge). */
export async function permissionSnapshot(): Promise<Record<string, unknown>> {
  try {
    return (await cmd<Record<string, unknown>>("permission_snapshot")) ?? {};
  } catch {
    const value = raw()?.permissionSnapshot?.();
    if (!value) return {};
    try {
      return JSON.parse(value) as Record<string, unknown>;
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

/** Native transcript history as a JSON string (array). */
export async function nativeTranscriptHistory(): Promise<string> {
  try {
    return (
      (await cmd<{ json?: string }>("native_transcript_history")).json ?? "[]"
    );
  } catch {
    return raw()?.nativeTranscriptHistory?.() ?? "[]";
  }
}

/** Delete a native transcript-history entry by id; returns the updated history JSON. */
export async function deleteHistoryEntry(id: number): Promise<string> {
  try {
    return (
      (await cmd<{ json?: string }>("delete_history_entry", { id })).json ??
      "[]"
    );
  } catch {
    return "[]";
  }
}

export async function syncTextFormatter(snapshot: string): Promise<void> {
  try {
    await cmd<void>("sync_text_formatter", { snapshot });
  } catch {
    raw()?.syncTextFormatter?.(snapshot);
  }
}

// ---- Bubble position ----

export async function bubbleCornerSnapshot(): Promise<string | undefined> {
  try {
    return (await cmd<{ value?: string }>("bubble_corner_snapshot")).value;
  } catch {
    return raw()?.bubbleCornerSnapshot?.();
  }
}

export async function setBubbleCorner(corner: string): Promise<void> {
  try {
    await cmd<void>("set_bubble_corner", { corner });
  } catch {
    raw()?.setBubbleCorner?.(corner);
  }
}

export async function engineDictationEnabled(): Promise<boolean> {
  try {
    return (
      (await cmd<{ value?: boolean }>("engine_dictation_enabled")).value ??
      false
    );
  } catch {
    return raw()?.engineDictationEnabled?.() ?? false;
  }
}

export async function setEngineDictationEnabled(
  enabled: boolean,
): Promise<boolean> {
  try {
    return (
      (
        await cmd<{ value?: boolean }>("set_engine_dictation_enabled", {
          enabled,
        })
      ).value ?? enabled
    );
  } catch {
    return raw()?.setEngineDictationEnabled?.(enabled) ?? enabled;
  }
}

export async function setEngineModelId(modelId: string): Promise<string> {
  try {
    return (
      (await cmd<{ value?: string }>("set_engine_model_id", { modelId }))
        .value ?? modelId
    );
  } catch {
    return raw()?.setEngineModelId?.(modelId) ?? modelId;
  }
}

export async function listAndroidAsrModelPacks(): Promise<
  AndroidAsrModelPackState[]
> {
  return appCmd<AndroidAsrModelPackState[]>("asr_list_model_packs");
}

export async function downloadAndroidAsrModelPack(
  modelId: string,
): Promise<void> {
  await appCmd<void>("asr_download_model_pack", { modelId });
}

export async function cancelAndroidAsrModelDownload(
  modelId: string,
): Promise<void> {
  await appCmd<void>("asr_cancel_model_download", { modelId });
}

export async function selectAndroidAsrModelPack(
  modelId: string,
): Promise<AndroidAsrModelPackState> {
  const state = await appCmd<AndroidAsrModelPackState>(
    "asr_select_model_pack",
    { modelId },
  );
  await setEngineModelId(state.installedDir || modelId);
  return state;
}

export async function deleteAndroidAsrModelPack(
  modelId: string,
): Promise<void> {
  await appCmd<void>("asr_delete_model_pack", { modelId });
}

export function onAndroidAsrModelProgress(
  cb: (progress: AndroidAsrDownloadProgress) => void,
): Promise<UnlistenFn> {
  return listen<AndroidAsrDownloadProgress>(
    "android-asr-model-progress",
    (event) => cb(event.payload),
  );
}

export function onAndroidAsrModelChanged(
  cb: (modelId: string) => void,
): Promise<UnlistenFn> {
  return listen<string>("android-asr-model-changed", (event) =>
    cb(event.payload),
  );
}

export async function startBubble(): Promise<void> {
  try {
    await cmd<void>("start_bubble");
  } catch {
    raw()?.startBubble?.();
  }
}

export async function stopBubble(): Promise<void> {
  try {
    await cmd<void>("stop_bubble");
  } catch {
    raw()?.stopBubble?.();
  }
}

// ---- Permission / settings entry points ----

export async function requestMicrophone(): Promise<void> {
  try {
    await cmd<void>("request_microphone");
  } catch {
    raw()?.requestMicrophone?.();
  }
}

export async function openOverlaySettings(): Promise<void> {
  try {
    await cmd<void>("open_overlay_settings");
  } catch {
    raw()?.openOverlaySettings?.();
  }
}

export async function openAccessibilitySettings(): Promise<void> {
  try {
    await cmd<void>("open_accessibility_settings");
  } catch {
    raw()?.openAccessibilitySettings?.();
  }
}

export async function requestSpeechModelDownload(): Promise<void> {
  try {
    await cmd<void>("request_speech_model_download");
  } catch {
    raw()?.requestSpeechModelDownload?.();
  }
}

export async function openExternalUrl(url: string): Promise<boolean> {
  try {
    return (
      (await cmd<{ value?: boolean }>("open_external_url", { url })).value ??
      false
    );
  } catch {
    return raw()?.openExternalUrl?.(url) ?? false;
  }
}
