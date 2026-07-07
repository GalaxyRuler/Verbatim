import React from "react";
import { useTranslation } from "react-i18next";
import { WordCorrectionThreshold } from "./WordCorrectionThreshold";
import { LogLevelSelector } from "./LogLevelSelector";
import { PasteDelay } from "./PasteDelay";
import { RecordingBuffer } from "./RecordingBuffer";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { LogDirectory } from "./LogDirectory";
import { DiagnosticsPanel } from "./DiagnosticsPanel";
import { DebugPaths } from "./DebugPaths";
import { useSettings } from "../../../hooks/useSettings";

export const DebugSettings: React.FC = () => {
  const { t } = useTranslation();
  const { settings } = useSettings();

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <SettingsGroup>
        <DiagnosticsPanel />
      </SettingsGroup>
      <SettingsGroup title={t("settings.debug.groups.logs")}>
        <LogDirectory grouped={true} />
        <LogLevelSelector grouped={true} />
      </SettingsGroup>
      {settings?.debug_mode && (
        <SettingsGroup title={t("settings.debug.title")}>
          <WordCorrectionThreshold descriptionMode="tooltip" grouped={true} />
          <PasteDelay descriptionMode="tooltip" grouped={true} />
          <RecordingBuffer descriptionMode="tooltip" grouped={true} />
          <DebugPaths descriptionMode="tooltip" grouped={true} />
        </SettingsGroup>
      )}
    </div>
  );
};
