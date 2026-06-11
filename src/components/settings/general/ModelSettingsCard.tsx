import React from "react";
import { useTranslation } from "react-i18next";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { LanguageSelector } from "../LanguageSelector";
import { TranslateToEnglish } from "../TranslateToEnglish";
import { useModelStore } from "../../../stores/modelStore";
import { useSettings } from "../../../hooks/useSettings";
import { translationSupportFromModel } from "../translationOptions";
import type { ModelInfo } from "@/bindings";

export const ModelSettingsCard: React.FC = () => {
  const { t } = useTranslation();
  const { currentModel, models } = useModelStore();
  const { getSetting } = useSettings();

  const selectedModel = currentModel || getSetting("selected_model") || "";
  const currentModelInfo = models.find(
    (m: ModelInfo) => m.id === selectedModel,
  );

  const supportsLanguageSelection =
    currentModelInfo?.supports_language_selection ?? false;
  const supportsTranslation = currentModelInfo?.supports_translation ?? false;

  // Don't render anything if no model is selected or no settings available
  if (!selectedModel || !currentModelInfo) {
    return null;
  }

  return (
    <SettingsGroup
      title={t("settings.modelSettings.title", {
        model: currentModelInfo.name,
      })}
    >
      {supportsLanguageSelection && (
        <LanguageSelector
          descriptionMode="tooltip"
          grouped={true}
          supportedLanguages={currentModelInfo.supported_languages}
        />
      )}
      <TranslateToEnglish
        descriptionMode="tooltip"
        grouped={true}
        disabled={!supportsTranslation}
        translationSupport={translationSupportFromModel(supportsTranslation)}
        description={
          supportsTranslation
            ? undefined
            : t("settings.advanced.translation.descriptionUnsupported", {
                model: currentModelInfo.name,
              })
        }
      />
    </SettingsGroup>
  );
};
