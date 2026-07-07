import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "../../hooks/useSettings";
import { Input } from "../ui/Input";
import { SettingContainer } from "../ui/SettingContainer";

interface HistoryLimitProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

export const HistoryLimit: React.FC<HistoryLimitProps> = ({
  descriptionMode = "inline",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  const historyLimit = getSetting("history_limit") ?? 5;

  const [invalid, setInvalid] = useState(false);

  const handleChange = async (event: React.ChangeEvent<HTMLInputElement>) => {
    const value = parseInt(event.target.value, 10);
    const ok = !isNaN(value) && value >= 0 && value <= 1000;
    setInvalid(!ok);
    if (ok) {
      updateSetting("history_limit", value);
    }
  };

  return (
    <SettingContainer
      title={t("settings.debug.historyLimit.title")}
      description={t("settings.debug.historyLimit.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
      layout="horizontal"
    >
      <div className="flex flex-col items-end gap-1">
        <div className="flex items-center space-x-2">
          <Input
            type="number"
            min="0"
            max="1000"
            value={historyLimit}
            onChange={handleChange}
            disabled={isUpdating("history_limit")}
            className="w-20"
          />
          <span className="text-sm text-text">
            {t("settings.debug.historyLimit.entries")}
          </span>
        </div>
        {invalid && (
          <p className="text-xs text-danger" role="alert">
            {t("settings.debug.historyLimit.invalid")}
          </p>
        )}
      </div>
    </SettingContainer>
  );
};
