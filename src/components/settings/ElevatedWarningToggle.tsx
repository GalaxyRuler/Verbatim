import React from "react";
import { useTranslation } from "react-i18next";
import { platform } from "@tauri-apps/plugin-os";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface ElevatedWarningToggleProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

// Windows-only: the underlying watcher (UIPI integrity isolation) is a no-op on
// macOS/Linux, so the toggle is irrelevant there.
export const ElevatedWarningToggle: React.FC<ElevatedWarningToggleProps> =
  React.memo(({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    if (platform() !== "windows") {
      return null;
    }

    const enabled = getSetting("warn_on_elevated_target") ?? true;

    return (
      <ToggleSwitch
        checked={enabled}
        onChange={(value) => updateSetting("warn_on_elevated_target", value)}
        isUpdating={isUpdating("warn_on_elevated_target")}
        label={t("settings.advanced.elevatedWarning.label")}
        description={t("settings.advanced.elevatedWarning.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      />
    );
  });
