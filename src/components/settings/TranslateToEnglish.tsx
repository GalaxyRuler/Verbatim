import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface TranslateToEnglishProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  disabled?: boolean;
  description?: string;
}

export const TranslateToEnglish: React.FC<TranslateToEnglishProps> = React.memo(
  ({
    descriptionMode = "tooltip",
    grouped = false,
    disabled = false,
    description,
  }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const translateToEnglish = getSetting("translate_to_english") || false;

    return (
      <ToggleSwitch
        checked={disabled ? false : translateToEnglish}
        onChange={(enabled) => updateSetting("translate_to_english", enabled)}
        disabled={disabled}
        isUpdating={isUpdating("translate_to_english")}
        label={t("settings.advanced.translateToEnglish.label")}
        description={
          description || t("settings.advanced.translateToEnglish.description")
        }
        descriptionMode={descriptionMode}
        grouped={grouped}
      />
    );
  },
);
