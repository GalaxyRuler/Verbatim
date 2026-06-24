import React from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "../../hooks/useSettings";
import { ToggleSwitch } from "../ui/ToggleSwitch";

interface HistoryStorageToggleProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const HistoryStorageToggle: React.FC<HistoryStorageToggleProps> =
  React.memo(({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();
    const enabled = getSetting("history_enabled") ?? true;

    return (
      <ToggleSwitch
        checked={enabled}
        onChange={(checked) => updateSetting("history_enabled", checked)}
        isUpdating={isUpdating("history_enabled")}
        label={t("settings.debug.historyStorage.label")}
        description={t("settings.debug.historyStorage.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      />
    );
  });

HistoryStorageToggle.displayName = "HistoryStorageToggle";

interface RecordingStorageToggleProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const RecordingStorageToggle: React.FC<RecordingStorageToggleProps> =
  React.memo(({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();
    const historyEnabled = getSetting("history_enabled") ?? true;
    const enabled = getSetting("recordings_enabled") ?? true;

    return (
      <ToggleSwitch
        checked={historyEnabled && enabled}
        onChange={(checked) => updateSetting("recordings_enabled", checked)}
        disabled={!historyEnabled}
        isUpdating={isUpdating("recordings_enabled")}
        label={t("settings.debug.recordingStorage.label")}
        description={
          historyEnabled
            ? t("settings.debug.recordingStorage.description")
            : t("settings.debug.recordingStorage.disabledDescription")
        }
        descriptionMode={descriptionMode}
        grouped={grouped}
      />
    );
  });

RecordingStorageToggle.displayName = "RecordingStorageToggle";
