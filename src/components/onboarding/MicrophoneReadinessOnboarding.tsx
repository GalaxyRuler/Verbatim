import React, { useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Mic } from "lucide-react";
import { useTranslation } from "react-i18next";
import { commands } from "@/bindings";
import { Alert } from "../ui/Alert";
import { Button } from "../ui/Button";
import VerbatimTextLogo from "../icons/VerbatimTextLogo";
import { MicrophoneSelector } from "../settings/MicrophoneSelector";
import { useSettings } from "../../hooks/useSettings";

interface MicrophoneReadinessOnboardingProps {
  onComplete: () => void;
}

const LEVEL_BAR_COUNT = 16;
const AUDIBLE_LEVEL_THRESHOLD = 0.03;

const normalizeLevel = (level: number) => Math.max(0, Math.min(1, level));

const MicrophoneReadinessOnboarding: React.FC<
  MicrophoneReadinessOnboardingProps
> = ({ onComplete }) => {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  const [levels, setLevels] = useState<number[]>(
    Array(LEVEL_BAR_COUNT).fill(0),
  );
  const [isTesting, setIsTesting] = useState(false);
  const [streamOpened, setStreamOpened] = useState(false);
  const [heardInput, setHeardInput] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const testActiveRef = useRef(false);

  const selectedMicrophone =
    getSetting("selected_microphone") === "default"
      ? "Default"
      : getSetting("selected_microphone") || "Default";

  const peakLevel = useMemo(
    () => Math.max(0, ...levels.map(normalizeLevel)),
    [levels],
  );

  useEffect(() => {
    const unlisten = listen<number[]>("mic-level", (event) => {
      const nextLevels = event.payload.slice(0, LEVEL_BAR_COUNT);
      while (nextLevels.length < LEVEL_BAR_COUNT) {
        nextLevels.push(0);
      }
      setLevels(nextLevels);
      if (Math.max(0, ...nextLevels) >= AUDIBLE_LEVEL_THRESHOLD) {
        setHeardInput(true);
      }
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    return () => {
      if (testActiveRef.current) {
        void commands.stopMicrophoneTest();
      }
    };
  }, []);

  const startTest = async () => {
    setError(null);
    setHeardInput(false);
    setLevels(Array(LEVEL_BAR_COUNT).fill(0));

    const result = await commands.startMicrophoneTest();
    if (result.status === "error") {
      setIsTesting(false);
      setStreamOpened(false);
      testActiveRef.current = false;
      setError(String(result.error));
      return;
    }

    testActiveRef.current = true;
    setIsTesting(result.data.stream_open);
    setStreamOpened(result.data.stream_open);
  };

  const stopTest = async () => {
    testActiveRef.current = false;
    setIsTesting(false);
    await commands.stopMicrophoneTest();
  };

  const complete = async () => {
    if (testActiveRef.current) {
      await stopTest();
    }
    onComplete();
  };

  return (
    <div className="h-screen w-screen flex flex-col p-6 gap-6 items-center justify-center">
      <div className="flex flex-col items-center gap-2">
        <VerbatimTextLogo width={200} />
      </div>

      <div className="max-w-xl w-full flex flex-col gap-4">
        <div className="text-center">
          <h2 className="text-xl font-semibold text-text mb-2">
            {t("onboarding.microphoneTest.title")}
          </h2>
          <p className="text-text/70">
            {t("onboarding.microphoneTest.description")}
          </p>
        </div>

        <div className="rounded-lg border border-mid-gray/20 bg-white/5 p-4">
          <MicrophoneSelector descriptionMode="inline" grouped={true} />
        </div>

        <div className="rounded-lg border border-mid-gray/20 bg-white/5 p-4 flex flex-col gap-4">
          <div className="flex items-center justify-between gap-3">
            <div className="flex items-center gap-3 min-w-0">
              <div className="p-2 rounded-full bg-logo-primary/20 shrink-0">
                <Mic className="w-5 h-5 text-logo-primary" />
              </div>
              <div className="min-w-0">
                <p className="text-sm font-medium text-text truncate">
                  {selectedMicrophone}
                </p>
                <p className="text-xs text-text/60">
                  {isTesting
                    ? t("onboarding.microphoneTest.listening")
                    : t("onboarding.microphoneTest.idle")}
                </p>
              </div>
            </div>
            <Button
              variant={isTesting ? "secondary" : "primary"}
              onClick={() => {
                void (isTesting ? stopTest() : startTest());
              }}
            >
              {isTesting
                ? t("onboarding.microphoneTest.stop")
                : t("onboarding.microphoneTest.start")}
            </Button>
          </div>

          <div
            role="meter"
            aria-label={t("onboarding.microphoneTest.meterLabel")}
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={Math.round(peakLevel * 100)}
            className="grid gap-1 h-12"
            style={{
              gridTemplateColumns: `repeat(${LEVEL_BAR_COUNT}, minmax(0, 1fr))`,
            }}
          >
            {levels.map((level, index) => (
              <div
                key={index}
                className="self-end rounded-md bg-logo-primary transition-all"
                style={{
                  height: `${Math.max(8, normalizeLevel(level) * 100)}%`,
                  opacity: 0.25 + normalizeLevel(level) * 0.75,
                }}
              />
            ))}
          </div>
        </div>

        {error && <Alert variant="error">{error}</Alert>}
        {streamOpened && !heardInput && (
          <Alert variant="info">{t("onboarding.microphoneTest.opened")}</Alert>
        )}
        {heardInput && (
          <Alert variant="success">
            {t("onboarding.microphoneTest.ready")}
          </Alert>
        )}

        <div className="flex justify-end gap-2">
          <Button
            variant="ghost"
            onClick={() => {
              void complete();
            }}
          >
            {t("onboarding.skipForNow")}
          </Button>
          <Button
            variant="primary"
            disabled={!streamOpened}
            onClick={() => {
              void complete();
            }}
          >
            {t("onboarding.microphoneTest.continue")}
          </Button>
        </div>
      </div>
    </div>
  );
};

export default MicrophoneReadinessOnboarding;
