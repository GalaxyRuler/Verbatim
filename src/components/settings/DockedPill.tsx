import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface DockedPillProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const DockedPill: React.FC<DockedPillProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const enabled = getSetting("docked_pill_enabled") || false;

    return (
      <ToggleSwitch
        checked={enabled}
        onChange={(enabled) => updateSetting("docked_pill_enabled", enabled)}
        isUpdating={isUpdating("docked_pill_enabled")}
        label={t("settings.advanced.dockedPill.label")}
        description={t("settings.advanced.dockedPill.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      />
    );
  },
);
