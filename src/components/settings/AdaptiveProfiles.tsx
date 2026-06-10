import React, { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { RotateCcw } from "lucide-react";
import { toast } from "sonner";
import type { AdaptiveProfile } from "@/bindings";
import { commands } from "@/bindings";
import { useSettings } from "../../hooks/useSettings";
import { Button } from "../ui/Button";
import { Dropdown, type DropdownOption } from "../ui/Dropdown";
import { Input } from "../ui/Input";
import { SettingContainer } from "../ui/SettingContainer";
import { ToggleSwitch } from "../ui/ToggleSwitch";

interface AdaptiveProfilesProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

const normalizeLanguageShortlist = (value: string): string[] =>
  value
    .split(",")
    .map((language) => language.trim().toLowerCase())
    .filter(Boolean);

export const AdaptiveProfiles: React.FC<AdaptiveProfilesProps> = ({
  descriptionMode = "tooltip",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const { settings, updateSetting, isUpdating, refreshSettings } =
    useSettings();
  const [profiles, setProfiles] = useState<AdaptiveProfile[]>([]);
  const [languageText, setLanguageText] = useState("");
  const [isCommandRunning, setCommandRunning] = useState(false);

  const languageShortlist = useMemo(
    () => settings?.adaptive_language_shortlist ?? [],
    [settings?.adaptive_language_shortlist],
  );

  useEffect(() => {
    setLanguageText(languageShortlist.join(", "));
  }, [languageShortlist]);

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

  const handleLanguageBlur = async () => {
    const nextShortlist = normalizeLanguageShortlist(languageText);
    if (
      nextShortlist.join(",") ===
      languageShortlist.map((item) => item).join(",")
    ) {
      return;
    }

    await updateSetting("adaptive_language_shortlist", nextShortlist);
  };

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
        toast.error(String(result.error));
      } else {
        await refreshSettings();
        toast.success(
          t("settings.advanced.adaptiveProfiles.actions.reprocessDone"),
        );
      }
    } catch (error) {
      toast.error(String(error));
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

      <SettingContainer
        title={t("settings.advanced.adaptiveProfiles.languages.title")}
        description={t(
          "settings.advanced.adaptiveProfiles.languages.description",
        )}
        descriptionMode={descriptionMode}
        grouped={grouped}
      >
        <Input
          className="w-[200px]"
          value={languageText}
          onChange={(event) => setLanguageText(event.target.value)}
          onBlur={handleLanguageBlur}
          disabled={isUpdating("adaptive_language_shortlist")}
          placeholder={t(
            "settings.advanced.adaptiveProfiles.languages.placeholder",
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
