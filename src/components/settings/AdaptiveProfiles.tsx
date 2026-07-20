import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { RotateCcw } from "lucide-react";
import { toast } from "sonner";
import type { AdaptiveProfile } from "@/bindings";
import { commands } from "@/bindings";
import { useSettings } from "../../hooks/useSettings";
import { Button } from "../ui/Button";
import { Dropdown, type DropdownOption } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { ToggleSwitch } from "../ui/ToggleSwitch";

interface AdaptiveProfilesProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

const PRIVATE_SESSION_REPROCESS_ERROR = "private_session_active";

export const AdaptiveProfiles: React.FC<AdaptiveProfilesProps> = ({
  descriptionMode = "tooltip",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const { settings, updateSetting, isUpdating, refreshSettings } =
    useSettings();
  const [profiles, setProfiles] = useState<AdaptiveProfile[]>([]);
  const [isCommandRunning, setCommandRunning] = useState(false);

  const reprocessErrorMessage = (error: unknown) => {
    const rawError = error instanceof Error ? error.message : String(error);
    return rawError === PRIVATE_SESSION_REPROCESS_ERROR
      ? t("settings.advanced.adaptiveProfiles.actions.privateSessionBlocked")
      : rawError;
  };

  useEffect(() => {
    let cancelled = false;

    const loadProfiles = async () => {
      const result = await commands.getAdaptiveProfiles();
      if (!cancelled && result.status === "ok") {
        setProfiles(result.data);
      }
    };

    loadProfiles();

    return () => {
      cancelled = true;
    };
  }, []);

  const profileOptions: DropdownOption[] = profiles.map((profile) => ({
    value: profile.id,
    label: profile.name,
  }));

  const handleResetCorrectionMemory = async () => {
    setCommandRunning(true);
    try {
      const result = await commands.resetAdaptiveCorrectionMemory();
      if (result.status === "error") {
        toast.error(String(result.error));
      } else {
        toast.success(
          t("settings.advanced.adaptiveProfiles.actions.resetDone"),
        );
      }
    } catch (error) {
      toast.error(String(error));
    } finally {
      setCommandRunning(false);
    }
  };

  const handleReprocessLast = async () => {
    setCommandRunning(true);
    try {
      const result = await commands.reprocessLastAdaptiveEntry(null);
      if (result.status === "error") {
        toast.error(reprocessErrorMessage(result.error));
      } else {
        await refreshSettings();
        toast.success(
          t("settings.advanced.adaptiveProfiles.actions.reprocessDone"),
        );
      }
    } catch (error) {
      toast.error(reprocessErrorMessage(error));
    } finally {
      setCommandRunning(false);
    }
  };

  return (
    <>
      <ToggleSwitch
        checked={settings?.adaptive_profiles_enabled ?? false}
        onChange={(enabled) =>
          updateSetting("adaptive_profiles_enabled", enabled)
        }
        isUpdating={isUpdating("adaptive_profiles_enabled")}
        label={t("settings.advanced.adaptiveProfiles.enabled.label")}
        description={t(
          "settings.advanced.adaptiveProfiles.enabled.description",
        )}
        descriptionMode={descriptionMode}
        grouped={grouped}
      />

      <ToggleSwitch
        checked={settings?.context_awareness_enabled ?? false}
        onChange={(enabled) =>
          updateSetting("context_awareness_enabled", enabled)
        }
        isUpdating={isUpdating("context_awareness_enabled")}
        label={t("settings.advanced.adaptiveProfiles.contextAwareness.label")}
        description={t(
          "settings.advanced.adaptiveProfiles.contextAwareness.description",
        )}
        descriptionMode={descriptionMode}
        grouped={grouped}
      />

      <ToggleSwitch
        checked={settings?.context_nearby_text_enabled ?? false}
        onChange={(enabled) =>
          updateSetting("context_nearby_text_enabled", enabled)
        }
        disabled={!settings?.context_awareness_enabled}
        isUpdating={isUpdating("context_nearby_text_enabled")}
        label={t("settings.advanced.adaptiveProfiles.nearbyText.label")}
        description={t(
          "settings.advanced.adaptiveProfiles.nearbyText.description",
        )}
        descriptionMode={descriptionMode}
        grouped={grouped}
      />

      <SettingContainer
        title={t("settings.advanced.adaptiveProfiles.defaultProfile.title")}
        description={t(
          "settings.advanced.adaptiveProfiles.defaultProfile.description",
        )}
        descriptionMode={descriptionMode}
        grouped={grouped}
      >
        <Dropdown
          options={profileOptions}
          selectedValue={settings?.adaptive_default_profile_id ?? null}
          onSelect={(profileId) =>
            updateSetting("adaptive_default_profile_id", profileId)
          }
          disabled={isUpdating("adaptive_default_profile_id")}
          placeholder={t(
            "settings.advanced.adaptiveProfiles.defaultProfile.placeholder",
          )}
        />
      </SettingContainer>

      <ToggleSwitch
        checked={settings?.adaptive_correction_memory_enabled ?? true}
        onChange={(enabled) =>
          updateSetting("adaptive_correction_memory_enabled", enabled)
        }
        isUpdating={isUpdating("adaptive_correction_memory_enabled")}
        label={t("settings.advanced.adaptiveProfiles.correctionMemory.label")}
        description={t(
          "settings.advanced.adaptiveProfiles.correctionMemory.description",
        )}
        descriptionMode={descriptionMode}
        grouped={grouped}
      />

      <SettingContainer
        title={t("settings.advanced.adaptiveProfiles.actions.title")}
        description={t(
          "settings.advanced.adaptiveProfiles.actions.description",
        )}
        descriptionMode={descriptionMode}
        grouped={grouped}
        layout="stacked"
      >
        <div className="flex flex-wrap gap-2">
          <Button
            type="button"
            variant="secondary"
            size="sm"
            onClick={handleReprocessLast}
            disabled={isCommandRunning}
            className="inline-flex items-center gap-2"
          >
            <RotateCcw className="h-3.5 w-3.5" />
            {t("settings.advanced.adaptiveProfiles.actions.reprocess")}
          </Button>
          <Button
            type="button"
            variant="secondary"
            size="sm"
            onClick={handleResetCorrectionMemory}
            disabled={isCommandRunning}
            className="inline-flex items-center gap-2"
          >
            <RotateCcw className="h-3.5 w-3.5" />
            {t("settings.advanced.adaptiveProfiles.actions.reset")}
          </Button>
        </div>
      </SettingContainer>
    </>
  );
};

AdaptiveProfiles.displayName = "AdaptiveProfiles";
