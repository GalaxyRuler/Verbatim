import React, { useCallback, useEffect, useMemo, useState } from "react";
import { RefreshCcw } from "lucide-react";
import { useTranslation } from "react-i18next";
import { checkAccessibilityPermission } from "tauri-plugin-macos-permissions-api";
import {
  commands,
  type AvailableAccelerators,
  type CredentialStoreStatus,
  type LinuxEnvironmentStatus,
  type ModelInfo,
  type PrivateSessionStatus,
  type ShortcutBinding,
  type StartupStatus,
  type WindowsMicrophonePermissionStatus,
} from "@/bindings";
import { useOsType } from "@/hooks/useOsType";
import { useSettings } from "@/hooks/useSettings";
import { getTranslatedModelName } from "@/lib/utils/modelTranslation";
import { Button } from "../../ui/Button";
import { PathDisplay } from "../../ui/PathDisplay";
import { SettingContainer } from "../../ui/SettingContainer";

type StatusTone = "success" | "warning" | "neutral" | "danger";

interface DiagnosticItem {
  id: string;
  label: string;
  value: string;
  detail?: string;
  tone?: StatusTone;
}

interface DiagnosticPaths {
  appDir?: string;
  appDirError?: string;
  logDir?: string;
  logDirError?: string;
}

interface DiagnosticState {
  startup?: StartupStatus;
  credentialStore?: CredentialStoreStatus;
  linux?: LinuxEnvironmentStatus;
  windowsMicrophone?: WindowsMicrophonePermissionStatus;
  macAccessibility?: boolean;
  accelerators?: AvailableAccelerators;
  selectedModel?: ModelInfo | null;
  privateSession?: PrivateSessionStatus;
  paths: DiagnosticPaths;
}

const toneClasses: Record<StatusTone, string> = {
  success: "text-success",
  warning: "text-warning",
  danger: "text-danger",
  neutral: "text-text-secondary",
};

const formatList = (items: string[], fallback: string): string =>
  items.length > 0 ? items.join(", ") : fallback;

const isShortcutBinding = (
  binding: ShortcutBinding | undefined,
): binding is ShortcutBinding => binding !== undefined;

export const DiagnosticsPanel: React.FC = () => {
  const { t } = useTranslation();
  const osType = useOsType();
  const { settings } = useSettings();
  const [diagnostics, setDiagnostics] = useState<DiagnosticState>({
    paths: {},
  });
  const [isLoading, setIsLoading] = useState(false);

  const selectedModelId = settings?.selected_model ?? "";

  const loadDiagnostics = useCallback(async () => {
    setIsLoading(true);

    const [
      startup,
      credentialStore,
      accelerators,
      appDir,
      logDir,
      linux,
      windowsMicrophone,
      macAccessibility,
      selectedModel,
      privateSession,
    ] = await Promise.allSettled([
      commands.getStartupStatus(),
      commands.getCredentialStoreStatus(),
      commands.getAvailableAccelerators(),
      commands.getAppDirPath(),
      commands.getLogDirPath(),
      osType === "linux"
        ? commands.getLinuxEnvironmentStatus()
        : Promise.resolve(undefined),
      osType === "windows"
        ? commands.getWindowsMicrophonePermissionStatus()
        : Promise.resolve(undefined),
      osType === "macos"
        ? checkAccessibilityPermission()
        : Promise.resolve(undefined),
      selectedModelId
        ? commands.getModelInfo(selectedModelId)
        : Promise.resolve({ status: "ok" as const, data: null }),
      commands.getPrivateSessionStatus(),
    ]);

    setDiagnostics({
      startup: startup.status === "fulfilled" ? startup.value : undefined,
      credentialStore:
        credentialStore.status === "fulfilled"
          ? credentialStore.value
          : undefined,
      accelerators:
        accelerators.status === "fulfilled" ? accelerators.value : undefined,
      linux: linux.status === "fulfilled" ? linux.value : undefined,
      windowsMicrophone:
        windowsMicrophone.status === "fulfilled"
          ? windowsMicrophone.value
          : undefined,
      macAccessibility:
        macAccessibility.status === "fulfilled"
          ? macAccessibility.value
          : undefined,
      selectedModel:
        selectedModel.status === "fulfilled" &&
        selectedModel.value.status === "ok"
          ? selectedModel.value.data
          : undefined,
      privateSession:
        privateSession.status === "fulfilled" &&
        privateSession.value.status === "ok"
          ? privateSession.value.data
          : undefined,
      paths: {
        appDir:
          appDir.status === "fulfilled" && appDir.value.status === "ok"
            ? appDir.value.data
            : undefined,
        appDirError:
          appDir.status === "fulfilled" && appDir.value.status === "error"
            ? String(appDir.value.error)
            : appDir.status === "rejected"
              ? String(appDir.reason)
              : undefined,
        logDir:
          logDir.status === "fulfilled" && logDir.value.status === "ok"
            ? logDir.value.data
            : undefined,
        logDirError:
          logDir.status === "fulfilled" && logDir.value.status === "error"
            ? String(logDir.value.error)
            : logDir.status === "rejected"
              ? String(logDir.reason)
              : undefined,
      },
    });
    setIsLoading(false);
  }, [osType, selectedModelId]);

  useEffect(() => {
    void loadDiagnostics();
  }, [loadDiagnostics]);

  const items = useMemo<DiagnosticItem[]>(() => {
    const unavailable = t("settings.debug.diagnostics.values.unavailable");
    const enabled = t("common.enabled");
    const disabled = t("common.disabled");
    const none = t("settings.debug.diagnostics.values.none");
    const permissionLabels = {
      allowed: t("settings.debug.diagnostics.permission.allowed"),
      denied: t("settings.debug.diagnostics.permission.denied"),
      unknown: t("settings.debug.diagnostics.permission.unknown"),
    };
    const items: DiagnosticItem[] = [];

    if (diagnostics.startup?.status === "ready") {
      items.push({
        id: "startup",
        label: t("settings.debug.diagnostics.rows.startup"),
        value: t("settings.debug.diagnostics.values.ready"),
        tone: "success",
      });
    } else if (diagnostics.startup?.status === "failed") {
      items.push({
        id: "startup",
        label: t("settings.debug.diagnostics.rows.startup"),
        value: t("settings.debug.diagnostics.values.failed"),
        detail: t("settings.debug.diagnostics.details.startupFailed", {
          step: diagnostics.startup.step,
          message: diagnostics.startup.message,
        }),
        tone: "danger",
      });
    } else {
      items.push({
        id: "startup",
        label: t("settings.debug.diagnostics.rows.startup"),
        value: t("settings.debug.diagnostics.values.starting"),
        tone: "warning",
      });
    }

    if (osType === "windows" && diagnostics.windowsMicrophone) {
      const access = diagnostics.windowsMicrophone.overall_access;
      items.push({
        id: "permissions",
        label: t("settings.debug.diagnostics.rows.permissions"),
        value: permissionLabels[access],
        detail: t("settings.debug.diagnostics.details.windowsMicrophone", {
          device: permissionLabels[diagnostics.windowsMicrophone.device_access],
          app: permissionLabels[diagnostics.windowsMicrophone.app_access],
          desktop:
            permissionLabels[diagnostics.windowsMicrophone.desktop_app_access],
        }),
        tone:
          access === "allowed"
            ? "success"
            : access === "denied"
              ? "danger"
              : "warning",
      });
    } else if (
      osType === "macos" &&
      diagnostics.macAccessibility !== undefined
    ) {
      items.push({
        id: "permissions",
        label: t("settings.debug.diagnostics.rows.permissions"),
        value: diagnostics.macAccessibility
          ? t("settings.debug.diagnostics.values.accessibilityGranted")
          : t("settings.debug.diagnostics.values.accessibilityMissing"),
        tone: diagnostics.macAccessibility ? "success" : "warning",
      });
    } else if (osType === "linux" && diagnostics.linux) {
      items.push({
        id: "permissions",
        label: t("settings.debug.diagnostics.rows.permissions"),
        value: diagnostics.linux.at_spi_available
          ? t("settings.debug.diagnostics.values.atSpiAvailable")
          : t("settings.debug.diagnostics.values.atSpiMissing"),
        detail: t("settings.debug.diagnostics.details.linuxSession", {
          session: diagnostics.linux.session_type,
          desktop: diagnostics.linux.desktop,
          tray: diagnostics.linux.tray_status,
        }),
        tone: diagnostics.linux.at_spi_available ? "success" : "warning",
      });
    } else {
      items.push({
        id: "permissions",
        label: t("settings.debug.diagnostics.rows.permissions"),
        value: unavailable,
        tone: "neutral",
      });
    }

    const bindings = Object.values(settings?.bindings ?? {}).filter(
      isShortcutBinding,
    );
    const assignedBindings = bindings.filter(
      (binding) => binding.current_binding.trim().length > 0,
    );
    items.push({
      id: "shortcuts",
      label: t("settings.debug.diagnostics.rows.shortcuts"),
      value: t("settings.debug.diagnostics.values.shortcuts", {
        assigned: assignedBindings.length,
        total: bindings.length,
      }),
      detail: assignedBindings
        .map((binding) => `${binding.name}: ${binding.current_binding}`)
        .join("; "),
      tone: assignedBindings.length > 0 ? "success" : "warning",
    });

    const selectedModelName = diagnostics.selectedModel
      ? getTranslatedModelName(diagnostics.selectedModel, t)
      : selectedModelId || unavailable;
    items.push({
      id: "model",
      label: t("settings.debug.diagnostics.rows.model"),
      value: selectedModelName,
      detail: diagnostics.selectedModel
        ? t("settings.debug.diagnostics.details.model", {
            engine: diagnostics.selectedModel.engine_type,
            downloaded: diagnostics.selectedModel.is_downloaded
              ? enabled
              : disabled,
            license: diagnostics.selectedModel.license_label,
            accelerators: formatList(
              diagnostics.selectedModel.accelerator_support,
              unavailable,
            ),
            integrity: diagnostics.selectedModel.sha256
              ? t("settings.debug.diagnostics.values.checksummed")
              : t("settings.debug.diagnostics.values.unverified"),
          })
        : undefined,
      tone: diagnostics.selectedModel?.sha256 ? "success" : "warning",
    });

    const gpuCount = diagnostics.accelerators?.gpu_devices.length ?? 0;
    items.push({
      id: "accelerator",
      label: t("settings.debug.diagnostics.rows.accelerator"),
      value: t("settings.debug.diagnostics.values.accelerator", {
        whisper: settings?.whisper_accelerator ?? "auto",
        ort: settings?.ort_accelerator ?? "auto",
        gpuCount,
      }),
      detail: diagnostics.accelerators
        ? t("settings.debug.diagnostics.details.accelerator", {
            ort: formatList(diagnostics.accelerators.ort, unavailable),
            devices: formatList(
              diagnostics.accelerators.gpu_devices.map((device) => device.name),
              none,
            ),
          })
        : undefined,
      tone: diagnostics.accelerators ? "success" : "neutral",
    });

    items.push({
      id: "insertion",
      label: t("settings.debug.diagnostics.rows.insertion"),
      value: settings?.paste_method ?? unavailable,
      detail: t("settings.debug.diagnostics.details.insertion", {
        clipboard: settings?.clipboard_handling ?? unavailable,
        typingTool: settings?.typing_tool ?? "auto",
        externalScript: settings?.external_script_path || none,
      }),
      tone: settings?.paste_method === "none" ? "warning" : "success",
    });

    items.push({
      id: "updates",
      label: t("settings.debug.diagnostics.rows.updates"),
      value: settings?.update_checks_enabled ? enabled : disabled,
      detail: t("settings.debug.diagnostics.details.updates"),
      tone: settings?.update_checks_enabled ? "success" : "neutral",
    });

    items.push({
      id: "storage",
      label: t("settings.debug.diagnostics.rows.storage"),
      value: t("settings.debug.diagnostics.values.storage", {
        history: settings?.history_enabled ? enabled : disabled,
        recordings: settings?.recordings_enabled ? enabled : disabled,
      }),
      detail: t("settings.debug.diagnostics.details.storage", {
        privateSession: diagnostics.privateSession?.enabled
          ? enabled
          : disabled,
        retention: settings?.recording_retention_period ?? "never",
      }),
      tone: settings?.history_enabled ? "success" : "neutral",
    });

    if (diagnostics.credentialStore) {
      items.push({
        id: "credentials",
        label: t("settings.debug.diagnostics.rows.credentials"),
        value: diagnostics.credentialStore.available ? enabled : disabled,
        detail:
          !diagnostics.credentialStore.available &&
          diagnostics.credentialStore.retained_legacy_api_key_count > 0
            ? t(
                "settings.debug.diagnostics.details.credentialStoreRetainedLegacy",
                {
                  count:
                    diagnostics.credentialStore.retained_legacy_api_key_count,
                },
              )
            : (diagnostics.credentialStore.message ??
              t("settings.debug.diagnostics.details.credentialStore", {
                platform: diagnostics.credentialStore.platform,
              })),
        tone: diagnostics.credentialStore.available ? "success" : "warning",
      });
    }

    if (osType === "linux" && diagnostics.linux) {
      const availableHelpers = diagnostics.linux.helpers
        .filter((helper) => helper.available)
        .map((helper) => helper.name);
      items.push({
        id: "linux-helpers",
        label: t("settings.debug.diagnostics.rows.linuxHelpers"),
        value: formatList(availableHelpers, none),
        detail:
          diagnostics.linux.warnings.length > 0
            ? diagnostics.linux.warnings.join("; ")
            : t("settings.debug.diagnostics.values.noWarnings"),
        tone: diagnostics.linux.warnings.length > 0 ? "warning" : "success",
      });
    }

    return items;
  }, [diagnostics, osType, selectedModelId, settings, t]);

  const openAppDir = () => {
    void commands.openAppDataDir();
  };

  const openLogDir = () => {
    void commands.openLogDir();
  };

  return (
    <SettingContainer
      title={t("settings.debug.diagnostics.title")}
      description={t("settings.debug.diagnostics.description")}
      descriptionMode="tooltip"
      grouped={true}
      layout="stacked"
    >
      <div className="space-y-4">
        <div className="flex justify-end">
          <Button
            type="button"
            variant="secondary"
            size="sm"
            onClick={() => void loadDiagnostics()}
            disabled={isLoading}
            className="inline-flex items-center gap-2"
          >
            <RefreshCcw
              className={`h-4 w-4 ${isLoading ? "animate-spin" : ""}`}
            />
            <span>{t("settings.debug.diagnostics.refresh")}</span>
          </Button>
        </div>

        <div className="overflow-hidden rounded-md border border-mid-gray/20">
          {items.map((item) => (
            <div
              key={item.id}
              className="grid grid-cols-[minmax(8rem,12rem)_1fr] gap-3 border-b border-mid-gray/20 px-3 py-2 last:border-b-0"
            >
              <div className="text-xs font-medium text-mid-gray">
                {item.label}
              </div>
              <div className="min-w-0 space-y-1">
                <span
                  className={`inline-flex max-w-full items-center gap-1.5 text-xs font-medium ${toneClasses[item.tone ?? "neutral"]}`}
                >
                  <span
                    aria-hidden
                    className="w-1.5 h-1.5 rounded-full bg-current shrink-0"
                  />
                  <span className="truncate">{item.value}</span>
                </span>
                {item.detail && (
                  <p className="break-words text-xs leading-snug text-text/60">
                    {item.detail}
                  </p>
                )}
              </div>
            </div>
          ))}
        </div>

        <div className="space-y-2">
          {diagnostics.paths.appDir ? (
            <PathDisplay path={diagnostics.paths.appDir} onOpen={openAppDir} />
          ) : (
            <p className="text-xs text-warning">
              {t("settings.debug.diagnostics.pathError", {
                error:
                  diagnostics.paths.appDirError ??
                  t("settings.debug.diagnostics.values.unavailable"),
              })}
            </p>
          )}
          {diagnostics.paths.logDir ? (
            <PathDisplay path={diagnostics.paths.logDir} onOpen={openLogDir} />
          ) : (
            <p className="text-xs text-warning">
              {t("settings.debug.diagnostics.pathError", {
                error:
                  diagnostics.paths.logDirError ??
                  t("settings.debug.diagnostics.values.unavailable"),
              })}
            </p>
          )}
        </div>
      </div>
    </SettingContainer>
  );
};
