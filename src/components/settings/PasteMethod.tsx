import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { Input } from "../ui/Input";
import { useSettings } from "../../hooks/useSettings";
import { useOsType } from "../../hooks/useOsType";
import {
  commands,
  type LinuxEnvironmentStatus,
  type PasteMethod,
} from "@/bindings";
import { Alert } from "../ui/Alert";

interface PasteMethodProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const PasteMethodSetting: React.FC<PasteMethodProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();
    const osType = useOsType();
    const [linuxStatus, setLinuxStatus] =
      useState<LinuxEnvironmentStatus | null>(null);
    const [linuxStatusFailed, setLinuxStatusFailed] = useState(false);

    useEffect(() => {
      if (osType !== "linux") {
        setLinuxStatus(null);
        setLinuxStatusFailed(false);
        return;
      }

      let isMounted = true;
      commands
        .getLinuxEnvironmentStatus()
        .then((status) => {
          if (!isMounted) return;
          setLinuxStatus(status);
          setLinuxStatusFailed(false);
        })
        .catch(() => {
          if (!isMounted) return;
          setLinuxStatus(null);
          setLinuxStatusFailed(true);
        });

      return () => {
        isMounted = false;
      };
    }, [osType]);

    const getPasteMethodOptions = (osType: string) => {
      const mod = osType === "macos" ? "Cmd" : "Ctrl";

      const options = [
        {
          value: "ctrl_v",
          label: t("settings.advanced.pasteMethod.options.clipboard", {
            modifier: mod,
          }),
        },
        {
          value: "direct",
          label: t("settings.advanced.pasteMethod.options.direct"),
        },
        {
          value: "none",
          label: t("settings.advanced.pasteMethod.options.none"),
        },
      ];

      // Add Shift+Insert and Ctrl+Shift+V options for Windows and Linux only
      if (osType === "windows" || osType === "linux") {
        options.push(
          {
            value: "ctrl_shift_v",
            label: t(
              "settings.advanced.pasteMethod.options.clipboardCtrlShiftV",
            ),
          },
          {
            value: "shift_insert",
            label: t(
              "settings.advanced.pasteMethod.options.clipboardShiftInsert",
            ),
          },
        );
      }

      // External script is only available on Linux
      if (osType === "linux") {
        options.push({
          value: "external_script",
          label: t("settings.advanced.pasteMethod.options.externalScript"),
        });
      }

      return options;
    };

    const selectedMethod = (getSetting("paste_method") ||
      "ctrl_v") as PasteMethod;
    const externalScriptPath = getSetting("external_script_path") || "";

    const pasteMethodOptions = getPasteMethodOptions(osType);
    const noLinuxHelper = t(
      "settings.advanced.pasteMethod.linuxReadiness.none",
    );

    return (
      <SettingContainer
        title={t("settings.advanced.pasteMethod.title")}
        description={t("settings.advanced.pasteMethod.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
        tooltipPosition="bottom"
      >
        <div className="flex flex-col gap-2">
          <Dropdown
            options={pasteMethodOptions}
            selectedValue={selectedMethod}
            onSelect={(value) =>
              updateSetting("paste_method", value as PasteMethod)
            }
            disabled={isUpdating("paste_method")}
          />
          {selectedMethod === "external_script" && (
            <Input
              type="text"
              value={externalScriptPath}
              onChange={(e) =>
                updateSetting("external_script_path", e.target.value)
              }
              placeholder={t(
                "settings.advanced.pasteMethod.externalScriptPlaceholder",
              )}
              disabled={isUpdating("external_script_path")}
            />
          )}
          {osType === "linux" && linuxStatusFailed && (
            <Alert variant="warning" contained>
              {t("settings.advanced.pasteMethod.linuxReadiness.unknown")}
            </Alert>
          )}
          {osType === "linux" && linuxStatus && (
            <Alert
              variant={linuxStatus.warnings.length > 0 ? "warning" : "success"}
              contained
            >
              {t(
                linuxStatus.warnings.length > 0
                  ? "settings.advanced.pasteMethod.linuxReadiness.limited"
                  : "settings.advanced.pasteMethod.linuxReadiness.ready",
                {
                  session: linuxStatus.session_type,
                  desktop: linuxStatus.desktop,
                  clipboardHelper:
                    linuxStatus.clipboard_helper ?? noLinuxHelper,
                  pasteHelper: linuxStatus.key_combo_helper ?? noLinuxHelper,
                  directHelper:
                    linuxStatus.direct_input_helper ?? noLinuxHelper,
                },
              )}
            </Alert>
          )}
        </div>
      </SettingContainer>
    );
  },
);
