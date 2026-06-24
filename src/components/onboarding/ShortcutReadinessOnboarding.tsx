import React from "react";
import { useTranslation } from "react-i18next";
import { Alert } from "../ui/Alert";
import { Button } from "../ui/Button";
import VerbatimTextLogo from "../icons/VerbatimTextLogo";
import { ShortcutInput } from "../settings/ShortcutInput";
import { useSettings } from "../../hooks/useSettings";

interface ShortcutReadinessOnboardingProps {
  onComplete: () => void;
}

export const shortcutReadinessIssue = (
  bindings: Record<
    string,
    { current_binding?: string | null; name?: string | null } | undefined
  >,
): "missing" | "duplicate" | null => {
  const transcribeBinding = bindings.transcribe?.current_binding?.trim() ?? "";
  if (!transcribeBinding) {
    return "missing";
  }

  const normalizedTranscribe = transcribeBinding.toLowerCase();
  const hasDuplicate = Object.entries(bindings).some(([id, binding]) => {
    if (id === "transcribe") return false;
    const currentBinding = binding?.current_binding?.trim().toLowerCase() ?? "";
    return currentBinding.length > 0 && currentBinding === normalizedTranscribe;
  });

  return hasDuplicate ? "duplicate" : null;
};

const ShortcutReadinessOnboarding: React.FC<
  ShortcutReadinessOnboardingProps
> = ({ onComplete }) => {
  const { t } = useTranslation();
  const { getSetting, isLoading } = useSettings();
  const bindings = getSetting("bindings") ?? {};
  const readinessIssue = shortcutReadinessIssue(bindings);
  const isReady = !isLoading && readinessIssue === null;

  return (
    <div className="h-screen w-screen flex flex-col p-6 gap-6 items-center justify-center">
      <div className="flex flex-col items-center gap-2">
        <VerbatimTextLogo width={200} />
      </div>

      <div className="max-w-xl w-full flex flex-col gap-4">
        <div className="text-center">
          <h2 className="text-xl font-semibold text-text mb-2">
            {t("onboarding.shortcut.title")}
          </h2>
          <p className="text-text/70">{t("onboarding.shortcut.description")}</p>
        </div>

        <div className="rounded-lg border border-mid-gray/20 bg-white/5 p-4">
          <ShortcutInput
            shortcutId="transcribe"
            descriptionMode="inline"
            grouped={true}
          />
        </div>

        {readinessIssue === "missing" && (
          <Alert variant="warning">{t("onboarding.shortcut.missing")}</Alert>
        )}
        {readinessIssue === "duplicate" && (
          <Alert variant="warning">{t("onboarding.shortcut.duplicate")}</Alert>
        )}
        {isReady && (
          <Alert variant="success">{t("onboarding.shortcut.ready")}</Alert>
        )}

        <div className="flex justify-end">
          <Button variant="primary" disabled={!isReady} onClick={onComplete}>
            {t("onboarding.shortcut.continue")}
          </Button>
        </div>
      </div>
    </div>
  );
};

export default ShortcutReadinessOnboarding;
