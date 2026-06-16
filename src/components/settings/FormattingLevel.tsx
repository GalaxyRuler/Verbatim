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

    const options: Array<{
      value: FormattingLevelValue;
      label: string;
      description: string;
    }> = [
      {
        value: "none",
        label: t("settings.formattingLevel.options.none"),
        description: t("settings.formattingLevel.optionDescriptions.none"),
      },
      {
        value: "light",
        label: t("settings.formattingLevel.options.light"),
        description: t("settings.formattingLevel.optionDescriptions.light"),
      },
      {
        value: "medium",
        label: t("settings.formattingLevel.options.medium"),
        description: t("settings.formattingLevel.optionDescriptions.medium"),
      },
      {
        value: "high",
        label: t("settings.formattingLevel.options.high"),
        description: t("settings.formattingLevel.optionDescriptions.high"),
      },
    ];
    const selectedDescription =
      options.find((option) => option.value === value)?.description ?? "";

    return (
      <SettingContainer
        title={t("settings.formattingLevel.title")}
        description={t("settings.formattingLevel.description")}
        descriptionMode={descriptionMode}
        layout="horizontal"
        grouped={grouped}
      >
        <div className="min-w-[220px] max-w-[260px]">
          <Dropdown
            selectedValue={value}
            options={options}
            onSelect={(level) =>
              updateSetting("formatting_level", level as FormattingLevelValue)
            }
            disabled={isUpdating("formatting_level")}
            className="w-full"
          />
          {selectedDescription && (
            <p className="mt-1 text-xs leading-snug text-mid-gray">
              {selectedDescription}
            </p>
          )}
        </div>
      </SettingContainer>
    );
  },
);

FormattingLevel.displayName = "FormattingLevel";
