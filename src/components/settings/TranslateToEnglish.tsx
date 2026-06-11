import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { Dropdown } from "../ui/Dropdown";
import { useSettings } from "../../hooks/useSettings";
import { LANGUAGES } from "../../lib/constants/languages";
import {
  ENGLISH_TRANSLATION_SUPPORT,
  isTranslationTargetSupported,
  translationTargetOptions,
  type TranslationSupport,
} from "./translationOptions";

interface TranslateToEnglishProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  disabled?: boolean;
  description?: string;
  translationSupport?: TranslationSupport;
}

export const TranslateToEnglish: React.FC<TranslateToEnglishProps> = React.memo(
  ({
    descriptionMode = "tooltip",
    grouped = false,
    disabled = false,
    description,
    translationSupport = ENGLISH_TRANSLATION_SUPPORT,
  }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const translationRequest = getSetting("translation_request");
    const translationEnabled =
      getSetting("translation_enabled") ??
      getSetting("translate_to_english") ??
      false;
    const targetLanguage = translationRequest?.target_language ?? "en";
    const supportedTargetLanguage = isTranslationTargetSupported(
      translationSupport,
      targetLanguage,
    )
      ? targetLanguage
      : "en";
    const targetOptions = translationTargetOptions(
      translationSupport,
      LANGUAGES,
    );
    const isTranslationUpdating =
      isUpdating("translation_enabled") ||
      isUpdating("translation_request") ||
      isUpdating("translate_to_english");

    const handleTargetLanguageSelect = async (selectedTarget: string) => {
      if (!isTranslationTargetSupported(translationSupport, selectedTarget)) {
        return;
      }

      await updateSetting("translation_request", {
        source_language: translationRequest?.source_language ?? "auto",
        target_language: selectedTarget,
        route: translationRequest?.route ?? "auto",
      });
    };

    return (
      <>
        <ToggleSwitch
          checked={disabled ? false : translationEnabled}
          onChange={(enabled) => updateSetting("translation_enabled", enabled)}
          disabled={disabled}
          isUpdating={isTranslationUpdating}
          label={t("settings.advanced.translation.label")}
          description={
            description || t("settings.advanced.translation.description")
          }
          descriptionMode={descriptionMode}
          grouped={grouped}
        />
        {!disabled && translationEnabled && targetOptions.length > 0 && (
          <div
            className={`flex items-center justify-between gap-3 px-4 p-2 ${
              grouped ? "" : "rounded-lg border border-mid-gray/20 mt-2"
            }`}
          >
            <div className="max-w-2/3">
              <h3 className="text-sm font-medium">
                {t("settings.advanced.translation.targetLanguage")}
              </h3>
              {descriptionMode === "inline" && (
                <p className="text-sm">
                  {t("settings.advanced.translation.targetLanguageDescription")}
                </p>
              )}
            </div>
            <Dropdown
              options={targetOptions}
              selectedValue={supportedTargetLanguage}
              onSelect={handleTargetLanguageSelect}
              placeholder={t("settings.advanced.translation.targetLanguage")}
              disabled={isTranslationUpdating}
            />
          </div>
        )}
      </>
    );
  },
);
