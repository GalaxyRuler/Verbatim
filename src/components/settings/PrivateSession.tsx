import React, { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import { commands, type PrivateSessionStatus } from "@/bindings";
import { ToggleSwitch } from "../ui/ToggleSwitch";

interface PrivateSessionToggleProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const PrivateSessionToggle: React.FC<PrivateSessionToggleProps> =
  React.memo(({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const [enabled, setEnabled] = useState(false);
    const [isUpdating, setIsUpdating] = useState(false);

    useEffect(() => {
      let cancelled = false;

      commands.getPrivateSessionStatus().then((result) => {
        if (!cancelled && result.status === "ok") {
          setEnabled(result.data.enabled);
        }
      });

      const unlisten = listen<PrivateSessionStatus>(
        "private-session-changed",
        (event) => {
          setEnabled(event.payload.enabled);
        },
      );

      return () => {
        cancelled = true;
        unlisten.then((fn) => fn());
      };
    }, []);

    const handleChange = async (checked: boolean) => {
      try {
        setIsUpdating(true);
        const result = await commands.setPrivateSessionEnabled(checked);
        if (result.status === "ok") {
          setEnabled(result.data.enabled);
        }
      } finally {
        setIsUpdating(false);
      }
    };

    return (
      <ToggleSwitch
        checked={enabled}
        onChange={handleChange}
        isUpdating={isUpdating}
        label={t("settings.debug.privateSession.label")}
        description={t("settings.debug.privateSession.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      />
    );
  });

PrivateSessionToggle.displayName = "PrivateSessionToggle";
