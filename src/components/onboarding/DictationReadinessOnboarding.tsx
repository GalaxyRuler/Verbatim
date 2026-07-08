import React, { useEffect, useRef, useState } from "react";
import { ClipboardCopy, RotateCcw, Square, Mic } from "lucide-react";
import { useTranslation } from "react-i18next";
import { commands } from "@/bindings";
import { Alert } from "../ui/Alert";
import { Button } from "../ui/Button";
import VerbatimTextLogo from "../icons/VerbatimTextLogo";

interface DictationReadinessOnboardingProps {
  onComplete: () => void;
}

type TestState = "idle" | "recording" | "transcribing" | "ready";

const DictationReadinessOnboarding: React.FC<
  DictationReadinessOnboardingProps
> = ({ onComplete }) => {
  const { t } = useTranslation();
  const [testState, setTestState] = useState<TestState>("idle");
  const [transcript, setTranscript] = useState("");
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const recordingRef = useRef(false);
  const startPendingRef = useRef(false);
  const abandonedRef = useRef(false);

  useEffect(() => {
    return () => {
      // If a start is still in flight, tell it to cancel once it resolves;
      // otherwise cancel an already-running recording directly.
      abandonedRef.current = true;
      if (recordingRef.current) {
        void commands.cancelOnboardingDictationTest();
      }
    };
  }, []);

  const startRecording = async () => {
    setError(null);
    setCopied(false);
    setTranscript("");

    // Fresh attempt: clear any abandonment left by a prior teardown (e.g. a
    // StrictMode mount/unmount/remount cycle) before awaiting the backend.
    abandonedRef.current = false;
    startPendingRef.current = true;
    try {
      const result = await commands.startOnboardingDictationTest();
      if (result.status === "error") {
        recordingRef.current = false;
        setTestState("idle");
        setError(String(result.error));
        return;
      }

      if (abandonedRef.current) {
        // The user advanced (skip/continue) or the view unmounted while the
        // start was pending. Cancel the recording the backend just began
        // instead of leaving it running behind the next step.
        recordingRef.current = false;
        void commands.cancelOnboardingDictationTest();
        return;
      }

      recordingRef.current = true;
      setTestState("recording");
    } finally {
      startPendingRef.current = false;
    }
  };

  const complete = async () => {
    if (startPendingRef.current) {
      // Start not resolved yet; startRecording will cancel when it does.
      abandonedRef.current = true;
    } else if (recordingRef.current) {
      recordingRef.current = false;
      await commands.cancelOnboardingDictationTest();
    }
    onComplete();
  };

  const stopRecording = async () => {
    recordingRef.current = false;
    setTestState("transcribing");

    const result = await commands.stopOnboardingDictationTest();
    if (result.status === "error") {
      setTestState("idle");
      setError(String(result.error));
      return;
    }

    setTranscript(result.data.text);
    setTestState("ready");
  };

  const copyTranscript = async () => {
    const result = await commands.copyOnboardingDictationText(transcript);
    if (result.status === "error" || !result.data) {
      setError(
        result.status === "error"
          ? String(result.error)
          : t("onboarding.dictationTest.copyFailed"),
      );
      return;
    }

    setCopied(true);
  };

  const reset = () => {
    setError(null);
    setCopied(false);
    setTranscript("");
    setTestState("idle");
  };

  const isBusy = testState === "recording" || testState === "transcribing";

  return (
    <div className="h-screen w-screen flex flex-col p-6 gap-6 items-center justify-center">
      <div className="flex flex-col items-center gap-2">
        <VerbatimTextLogo width={200} />
      </div>

      <div className="max-w-xl w-full flex flex-col gap-4">
        <div className="text-center">
          <h2 className="text-xl font-semibold text-text mb-2">
            {t("onboarding.dictationTest.title")}
          </h2>
          <p className="text-text/70">
            {t("onboarding.dictationTest.description")}
          </p>
        </div>

        <div className="rounded-lg border border-mid-gray/20 bg-white/5 p-4 flex flex-col gap-4">
          <div className="flex items-center gap-3">
            <div className="p-2 rounded-full bg-accent/20 shrink-0">
              <Mic className="w-5 h-5 text-text" />
            </div>
            <div>
              <p className="text-sm font-medium text-text">
                {t(`onboarding.dictationTest.states.${testState}`)}
              </p>
              <p className="text-xs text-text/60">
                {t("onboarding.dictationTest.samplePrompt")}
              </p>
            </div>
          </div>

          {testState === "ready" && (
            <div className="rounded-md border border-mid-gray/20 bg-background/60 p-3 text-sm text-text whitespace-pre-wrap">
              {transcript}
            </div>
          )}

          <div className="flex flex-wrap gap-2">
            {testState !== "recording" && testState !== "ready" && (
              <Button
                variant="primary"
                disabled={isBusy}
                onClick={() => {
                  void startRecording();
                }}
              >
                {t("onboarding.dictationTest.start")}
              </Button>
            )}

            {testState === "recording" && (
              <Button
                variant="primary"
                onClick={() => {
                  void stopRecording();
                }}
              >
                <span className="inline-flex items-center gap-2">
                  <Square className="h-3.5 w-3.5" />
                  {t("onboarding.dictationTest.stop")}
                </span>
              </Button>
            )}

            {testState === "ready" && (
              <>
                <Button
                  variant="secondary"
                  onClick={() => {
                    void copyTranscript();
                  }}
                >
                  <span className="inline-flex items-center gap-2">
                    <ClipboardCopy className="h-3.5 w-3.5" />
                    {t("onboarding.dictationTest.copy")}
                  </span>
                </Button>
                <Button variant="secondary" onClick={reset}>
                  <span className="inline-flex items-center gap-2">
                    <RotateCcw className="h-3.5 w-3.5" />
                    {t("onboarding.dictationTest.recordAgain")}
                  </span>
                </Button>
              </>
            )}
          </div>
        </div>

        {error && <Alert variant="error">{error}</Alert>}
        {testState === "transcribing" && (
          <Alert variant="info">
            {t("onboarding.dictationTest.transcribing")}
          </Alert>
        )}
        {copied && (
          <Alert variant="success">
            {t("onboarding.dictationTest.copied")}
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
            disabled={testState !== "ready"}
            onClick={() => {
              void complete();
            }}
          >
            {t("onboarding.dictationTest.discardContinue")}
          </Button>
        </div>
      </div>
    </div>
  );
};

export default DictationReadinessOnboarding;
