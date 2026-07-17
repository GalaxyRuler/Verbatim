import { useCallback, useEffect, useRef, useState } from "react";
import { toast, Toaster } from "sonner";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import { platform } from "@tauri-apps/plugin-os";
import { relaunch } from "@tauri-apps/plugin-process";
import {
  checkAccessibilityPermission,
  checkMicrophonePermission,
} from "tauri-plugin-macos-permissions-api";
import { ModelStateEvent, RecordingErrorEvent } from "./lib/types/events";
import "./App.css";
import AccessibilityPermissions from "./components/AccessibilityPermissions";
import Footer from "./components/footer";
import Onboarding, {
  AccessibilityOnboarding,
  DictationReadinessOnboarding,
  MicrophoneReadinessOnboarding,
  ShortcutReadinessOnboarding,
} from "./components/onboarding";
import { Sidebar, SidebarSection, SECTIONS_CONFIG } from "./components/Sidebar";
import VerbatimMark from "./components/icons/VerbatimMark";
import { useSettings } from "./hooks/useSettings";
import type { DictionaryEntry, StartupStatus } from "@/bindings";
import { useDictionaryStore } from "./stores/dictionaryStore";
import { useSettingsStore } from "./stores/settingsStore";
import { commands } from "@/bindings";
import { getLanguageDirection, initializeRTL } from "@/lib/utils/rtl";
import { Alert } from "./components/ui/Alert";
import { Button } from "./components/ui/Button";
import AndroidApp from "./android/AndroidApp";

type OnboardingStep =
  | "accessibility"
  | "model"
  | "shortcut"
  | "microphone"
  | "dictation"
  | "done";

type LanguageGuardBlockedEvent = {
  locked_language: string;
  preview: string;
};

type TransformSelectionCaptureBlockedEvent = {
  reason_code: "secure_field" | "secure_check_error";
};

type DictationBlockedEvent = {
  app_name: string;
};

type PasteRecoveryEvent = {
  reason?: "paste_failure" | "target_changed" | "language_guard";
  copied?: boolean;
  paste_here_available?: boolean;
};

type CoordinatorHealthEvent = {
  status: "restarted" | "disabled";
  restart_count: number;
  reason: string;
};

const renderSettingsContent = (section: SidebarSection) => {
  const ActiveComponent =
    SECTIONS_CONFIG[section]?.component || SECTIONS_CONFIG.general.component;
  return <ActiveComponent />;
};

function DesktopApp() {
  const { t, i18n } = useTranslation();
  const [onboardingStep, setOnboardingStep] = useState<OnboardingStep | null>(
    null,
  );
  // Track if this is a returning user who just needs to grant permissions
  // (vs a new user who needs full onboarding including model selection)
  const [isReturningUser, setIsReturningUser] = useState(false);
  const [currentSection, setCurrentSection] =
    useState<SidebarSection>("general");
  const { settings, updateSetting } = useSettings();
  const direction = getLanguageDirection(i18n.language);
  const refreshAudioDevices = useSettingsStore(
    (state) => state.refreshAudioDevices,
  );
  const refreshOutputDevices = useSettingsStore(
    (state) => state.refreshOutputDevices,
  );
  const refreshSettings = useSettingsStore((state) => state.refreshSettings);
  const setRecentlyLearnedDictionaryEntries = useDictionaryStore(
    (state) => state.setRecentlyLearnedEntries,
  );
  const loadDictionaryCandidates = useDictionaryStore(
    (state) => state.loadCandidates,
  );
  const hasCompletedPostOnboardingInit = useRef(false);
  const [startupStatus, setStartupStatus] = useState<StartupStatus | null>(
    null,
  );
  const [isResettingSettings, setIsResettingSettings] = useState(false);

  useEffect(() => {
    let cancelled = false;

    commands
      .getStartupStatus()
      .then((result) => {
        if (cancelled) return;
        setStartupStatus(result);
      })
      .catch((error) => {
        if (cancelled) return;
        setStartupStatus({
          status: "failed",
          step: t("errors.startupUnknownStep"),
          message: String(error),
        });
      });

    return () => {
      cancelled = true;
    };
  }, [t]);

  useEffect(() => {
    if (startupStatus?.status === "ready") {
      void checkOnboardingStatus();
    }
    if (startupStatus?.status === "failed") {
      setOnboardingStep(null);
    }
  }, [startupStatus]);

  useEffect(() => {
    const unlisten = listen<CoordinatorHealthEvent>(
      "transcription-coordinator-health",
      (event) => {
        if (event.payload.status === "disabled") {
          setStartupStatus({
            status: "failed",
            step: t("errors.coordinatorFailedStep"),
            message: t("errors.coordinatorFailedMessage"),
          });
          return;
        }

        toast.warning(t("errors.coordinatorRestartedTitle"), {
          description: t("errors.coordinatorRestartedDescription"),
        });
      },
    );
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [t]);

  // Initialize RTL direction when language changes
  useEffect(() => {
    initializeRTL(i18n.language);
  }, [i18n.language]);

  // Initialize Enigo, shortcuts, and refresh audio devices when main app loads
  useEffect(() => {
    if (onboardingStep === "done" && !hasCompletedPostOnboardingInit.current) {
      hasCompletedPostOnboardingInit.current = true;
      Promise.all([
        commands.initializeEnigo(),
        commands.initializeShortcuts(),
      ]).catch((e) => {
        console.warn("Failed to initialize:", e);
      });
      refreshAudioDevices();
      refreshOutputDevices();
    }
  }, [onboardingStep, refreshAudioDevices, refreshOutputDevices]);

  // Handle keyboard shortcuts for debug mode toggle
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      // Check for Ctrl+Shift+D (Windows/Linux) or Cmd+Shift+D (macOS)
      const isDebugShortcut =
        event.shiftKey &&
        event.key.toLowerCase() === "d" &&
        (event.ctrlKey || event.metaKey);

      if (isDebugShortcut) {
        event.preventDefault();
        const currentDebugMode = settings?.debug_mode ?? false;
        updateSetting("debug_mode", !currentDebugMode);
      }
    };

    // Add event listener when component mounts
    document.addEventListener("keydown", handleKeyDown);

    // Cleanup event listener when component unmounts
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [settings?.debug_mode, updateSetting]);

  // Listen for recording errors from the backend and show a toast
  useEffect(() => {
    const unlisten = listen<RecordingErrorEvent>("recording-error", (event) => {
      const { error_type, detail } = event.payload;

      if (error_type === "microphone_permission_denied") {
        const currentPlatform = platform();
        const platformKey = `errors.micPermissionDenied.${currentPlatform}`;
        const description = t(platformKey, {
          defaultValue: t("errors.micPermissionDenied.generic"),
        });
        toast.error(t("errors.micPermissionDeniedTitle"), { description });
      } else if (error_type === "no_input_device") {
        toast.error(t("errors.noInputDeviceTitle"), {
          description: t("errors.noInputDevice"),
        });
      } else if (error_type === "target_privacy_excluded") {
        toast.warning(t("errors.targetPrivacyExcludedTitle"), {
          description: t("errors.targetPrivacyExcludedDescription"),
        });
      } else {
        toast.error(
          t("errors.recordingFailed", { error: detail ?? "Unknown error" }),
        );
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [t]);

  // Listen for paste failures and show a toast.
  // The technical error detail is logged to verbatim.log on the Rust side
  // (see actions.rs `error!("Failed to paste transcription: ...")`),
  // so we show a localized, user-friendly message here instead of the raw error.
  useEffect(() => {
    const unlisten = listen<PasteRecoveryEvent | null>(
      "paste-error",
      (event) => {
        const recovery = event.payload ?? {
          reason: "paste_failure",
          copied: true,
          paste_here_available: false,
        };

        if (recovery.reason === "language_guard") {
          return;
        }

        if (recovery.reason === "target_changed") {
          toast.warning(t("errors.targetChangedTitle"), {
            duration: 10000,
            description: t("errors.targetChangedDescription"),
            action: {
              label: t("errors.targetChangedPasteHereAction"),
              onClick: () => {
                void commands.pasteLastTranscript();
              },
            },
          });
          return;
        }

        toast.error(t("errors.pasteFailedTitle"), {
          duration: Infinity,
          description: recovery.copied
            ? `${t("errors.pasteFailed")} ${t("errors.pasteFailedCopiedHint")}`
            : t("errors.pasteFailed"),
          action: {
            label: t("errors.pasteFailedCopyAction"),
            onClick: () => {
              void commands.copyLastTranscript();
            },
          },
        });
      },
    );
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [t]);

  useEffect(() => {
    const unlisten = listen("transform-recovery-copied", () => {
      toast.error(t("errors.transformRecoveryCopiedTitle"), {
        duration: Infinity,
        description: t("errors.transformRecoveryCopiedDescription"),
        action: {
          label: t("errors.pasteFailedCopyAction"),
          onClick: () => {
            void commands.copyLastTransformResult();
          },
        },
      });
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [t]);

  useEffect(() => {
    const unlisten = listen<TransformSelectionCaptureBlockedEvent>(
      "transform-selection-capture-blocked",
      (event) => {
        const description =
          event.payload.reason_code === "secure_check_error"
            ? t("errors.transformSecureCheckErrorDescription")
            : t("errors.transformSecureFieldDescription");

        toast.warning(t("errors.transformSecureFieldTitle"), { description });
      },
    );
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [t]);

  // If a locked language clearly conflicts with the transcribed script, keep
  // the text recoverable while making the recovery action explicit.
  useEffect(() => {
    const unlisten = listen<LanguageGuardBlockedEvent>(
      "language-guard-blocked",
      (event) => {
        const preview = event.payload.preview.trim();
        const descriptionParts = [
          t("errors.languageGuardDescription", {
            language: event.payload.locked_language,
          }),
        ];
        if (preview.length > 0) {
          descriptionParts.push(t("errors.languageGuardPreview", { preview }));
        }

        toast.warning(t("errors.languageGuardTitle"), {
          duration: 10000,
          description: descriptionParts.join(" "),
          action: {
            label: t("errors.languageGuardPasteAnyway"),
            onClick: () => {
              void commands.pasteLastTranscript();
            },
          },
        });
      },
    );
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [t]);

  // Surface a native OS notification when the focused app runs at a higher
  // integrity level than Verbatim (e.g. "run as administrator"), which makes
  // Windows silently block dictation there. Native (not in-app) because the
  // main window is usually hidden in the tray. Text is localized here.
  useEffect(() => {
    const unlisten = listen<DictationBlockedEvent>(
      "dictation-blocked-elevated",
      async (event) => {
        const app = event.payload.app_name;
        try {
          let granted = await isPermissionGranted();
          if (!granted) {
            granted = (await requestPermission()) === "granted";
          }
          if (!granted) return;
          sendNotification({
            title: t("notifications.elevatedWindow.title"),
            body: t("notifications.elevatedWindow.body", { app }),
          });
        } catch {
          // Notifications are best-effort; never break the app over a toast.
        }
      },
    );
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [t]);

  // Show feedback when post-paste dictionary learning adds corrected entries.
  useEffect(() => {
    const unlisten = listen<DictionaryEntry[]>(
      "dictionary-entries-learned",
      (event) => {
        if (event.payload.length === 0) return;

        void refreshSettings();
        setRecentlyLearnedDictionaryEntries(event.payload);
        window.dispatchEvent(
          new CustomEvent("verbatim-dictionary-entries-learned", {
            detail: event.payload,
          }),
        );
        toast.success(t("settings.dictionary.recentlyLearned.title"), {
          description: t("settings.dictionary.recentlyLearned.description", {
            phrases: event.payload.map((entry) => entry.phrase).join(", "),
          }),
        });
      },
    );
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [refreshSettings, setRecentlyLearnedDictionaryEntries, t]);

  // Refresh the quarantined-candidate list when post-paste learning stages
  // new (unconfirmed) phrases for review.
  useEffect(() => {
    const unlisten = listen<number>("dictionary-candidates-learned", () => {
      void loadDictionaryCandidates();
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [loadDictionaryCandidates]);

  useEffect(() => {
    const unlisten = listen("open-dictionary-settings", () => {
      setCurrentSection("dictionary");
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    const unlisten = listen("open-general-settings", () => {
      setCurrentSection("general");
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  // Listen for model loading failures and show a toast
  useEffect(() => {
    const unlisten = listen<ModelStateEvent>("model-state-changed", (event) => {
      if (event.payload.event_type === "loading_failed") {
        toast.error(
          t("errors.modelLoadFailed", {
            model:
              event.payload.model_name || t("errors.modelLoadFailedUnknown"),
          }),
          {
            description: event.payload.error,
          },
        );
      } else if (
        event.payload.event_type === "loading_completed" &&
        event.payload.fallback === "cpu_after_gpu_preflight_failed"
      ) {
        toast.warning(t("errors.gpuUnavailableUsingCpu"));
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [t]);

  const revealMainWindowForPermissions = async () => {
    try {
      await commands.showMainWindowCommand();
    } catch (e) {
      console.warn("Failed to show main window for permission onboarding:", e);
    }
  };

  const checkOnboardingStatus = async () => {
    try {
      // Check if they have any models available
      const result = await commands.hasAnyModelsAvailable();
      const hasModels = result.status === "ok" && result.data;
      const currentPlatform = platform();

      if (hasModels) {
        // Returning user - check if they need to grant permissions first
        setIsReturningUser(true);

        if (currentPlatform === "macos") {
          try {
            const [hasAccessibility, hasMicrophone] = await Promise.all([
              checkAccessibilityPermission(),
              checkMicrophonePermission(),
            ]);
            if (!hasAccessibility || !hasMicrophone) {
              await revealMainWindowForPermissions();
              setOnboardingStep("accessibility");
              return;
            }
          } catch (e) {
            console.warn("Failed to check macOS permissions:", e);
            // If we can't check, proceed to main app and let them fix it there
          }
        }

        if (currentPlatform === "windows") {
          try {
            const microphoneStatus =
              await commands.getWindowsMicrophonePermissionStatus();
            if (
              microphoneStatus.supported &&
              microphoneStatus.overall_access === "denied"
            ) {
              await revealMainWindowForPermissions();
              setOnboardingStep("accessibility");
              return;
            }
          } catch (e) {
            console.warn("Failed to check Windows microphone permissions:", e);
            // If we can't check, proceed to main app and let them fix it there
          }
        }

        setOnboardingStep("done");
      } else {
        // New user - start full onboarding
        setIsReturningUser(false);
        setOnboardingStep("accessibility");
      }
    } catch (error) {
      console.error("Failed to check onboarding status:", error);
      setOnboardingStep("accessibility");
    }
  };

  const handleAccessibilityComplete = useCallback(() => {
    // Returning users already have models, skip to main app
    // New users need to select a model
    setOnboardingStep(isReturningUser ? "done" : "model");
  }, [isReturningUser]);

  const handleModelSelected = useCallback(() => {
    // New users should confirm the recording shortcut before entering the app.
    setOnboardingStep("shortcut");
  }, []);

  const handleShortcutReadinessComplete = useCallback(() => {
    setOnboardingStep("microphone");
  }, []);

  const handleMicrophoneReadinessComplete = useCallback(() => {
    setOnboardingStep("dictation");
  }, []);

  const handleDictationReadinessComplete = useCallback(() => {
    setOnboardingStep("done");
  }, []);

  const handleRestart = () => {
    void relaunch();
  };

  const handleResetSettings = async () => {
    try {
      setIsResettingSettings(true);
      const result = await commands.resetSettingsToDefaults();
      if (result.status === "error") {
        throw new Error(String(result.error));
      }
      await relaunch();
    } catch (error) {
      setIsResettingSettings(false);
      toast.error(t("errors.startupResetSettingsFailed"), {
        description: String(error),
      });
    }
  };

  // Still checking onboarding status — show a quiet splash instead of a blank window
  if (startupStatus === null || startupStatus.status === "starting") {
    return (
      <div
        dir={direction}
        className="h-screen flex flex-col items-center justify-center gap-3 select-none cursor-default bg-background text-text"
      >
        <VerbatimMark width={40} height={40} aria-hidden />
        <p className="text-sm text-text-secondary">{t("common.starting")}</p>
      </div>
    );
  }

  if (startupStatus.status === "failed") {
    return (
      <div
        dir={direction}
        className="h-screen flex flex-col select-none cursor-default bg-background text-text"
      >
        <div className="flex-1 flex items-center justify-center p-6">
          <div className="w-full max-w-xl flex flex-col gap-4">
            <div>
              <h1 className="text-xl font-semibold">
                {t("errors.startupFailedTitle")}
              </h1>
              <p className="mt-2 text-sm text-text-secondary">
                {t("errors.startupFailedDescription")}
              </p>
            </div>
            <Alert variant="error">
              {t("errors.startupFailedStep", {
                step: startupStatus.step,
              })}
            </Alert>
            <p className="text-sm text-text-secondary break-words">
              {startupStatus.message}
            </p>
            <div className="flex flex-wrap gap-2">
              <Button variant="primary" onClick={handleRestart}>
                {t("errors.startupRestart")}
              </Button>
              <Button
                variant="secondary"
                onClick={() => {
                  void commands.openLogDir();
                }}
              >
                {t("errors.startupOpenLogs")}
              </Button>
              <Button
                variant="secondary"
                onClick={() => {
                  void commands.openAppDataDir();
                }}
              >
                {t("errors.startupOpenAppData")}
              </Button>
              <Button
                variant="danger-ghost"
                disabled={isResettingSettings}
                onClick={() => {
                  void handleResetSettings();
                }}
              >
                {isResettingSettings
                  ? t("errors.startupResettingSettings")
                  : t("errors.startupResetSettings")}
              </Button>
            </div>
          </div>
        </div>
      </div>
    );
  }

  if (onboardingStep === null) {
    return null;
  }

  if (onboardingStep === "accessibility") {
    return <AccessibilityOnboarding onComplete={handleAccessibilityComplete} />;
  }

  if (onboardingStep === "model") {
    return <Onboarding onModelSelected={handleModelSelected} />;
  }

  if (onboardingStep === "shortcut") {
    return (
      <ShortcutReadinessOnboarding
        onComplete={handleShortcutReadinessComplete}
      />
    );
  }

  if (onboardingStep === "microphone") {
    return (
      <MicrophoneReadinessOnboarding
        onComplete={handleMicrophoneReadinessComplete}
      />
    );
  }

  if (onboardingStep === "dictation") {
    return (
      <DictationReadinessOnboarding
        onComplete={handleDictationReadinessComplete}
      />
    );
  }

  return (
    <div
      dir={direction}
      className="h-screen flex flex-col select-none cursor-default"
    >
      <Toaster
        theme="system"
        closeButton
        toastOptions={{
          unstyled: true,
          classNames: {
            toast:
              "bg-background border border-border rounded-lg shadow-lg px-4 py-3 flex items-center gap-3 text-sm",
            title: "font-medium",
            description: "text-text-secondary",
            closeButton:
              "text-text-secondary hover:text-text border border-border rounded-full bg-background",
          },
        }}
      />
      {/* Main content area that takes remaining space */}
      <div className="flex-1 flex overflow-hidden">
        <Sidebar
          activeSection={currentSection}
          onSectionChange={setCurrentSection}
        />
        {/* Scrollable content area */}
        <div className="flex-1 flex flex-col overflow-hidden">
          <div className="flex-1 overflow-y-auto">
            <div className="flex flex-col items-center p-4 gap-4">
              <AccessibilityPermissions />
              {renderSettingsContent(currentSection)}
            </div>
          </div>
        </div>
      </div>
      {/* Fixed footer at bottom */}
      <Footer />
    </div>
  );
}

function App() {
  if (platform() === "android") {
    return <AndroidApp />;
  }

  return <DesktopApp />;
}

export default App;
