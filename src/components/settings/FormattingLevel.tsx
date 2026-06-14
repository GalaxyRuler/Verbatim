import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { useSettings } from "../../hooks/useSettings";
import type { FormattingLevel as FormattingLevelValue } from "@/bindings";

interface FormattingLevelProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const FormattingLevel: React.FC<FormattingLevelProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();
    const value = getSetting("formatting_level") || "light";

    const options: Array<{ value: FormattingLevelValue; label: string }> = [
      { value: "none", label: t("settings.formattingLevel.options.none") },
      { value: "light", label: t("settings.formattingLevel.options.light") },
      { value: "medium", label: t("settings.formattingLevel.options.medium") },
      { value: "high", label: t("settings.formattingLevel.options.high") },
    ];

    return (
      <SettingContainer
        title={t("settings.formattingLevel.title")}
        description={t("settings.formattingLevel.description")}
        descriptionMode={descriptionMode}
        layout="horizontal"
        grouped={grouped}
      >
        <Dropdown
          selectedValue={value}
          options={options}
          onSelect={(level) =>
            updateSetting("formatting_level", level as FormattingLevelValue)
          }
          disabled={isUpdating("formatting_level")}
          className="min-w-[220px]"
        />
      </SettingContainer>
    );
  },
);

FormattingLevel.displayName = "FormattingLevel";
