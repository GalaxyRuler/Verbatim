import { emit, listen } from "@tauri-apps/api/event";
import React, { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  BookOpen,
  ClipboardCopy,
  ClipboardPaste,
  RotateCcw,
  Settings,
  Undo2,
} from "lucide-react";
import {
  MicrophoneIcon,
  TranscriptionIcon,
  CancelIcon,
} from "../components/icons";
import "./RecordingOverlay.css";
import { commands, type DictionaryEntry } from "@/bindings";
import i18n, { syncLanguageFromSettings } from "@/i18n";
import {
  type DictationLanguageSelection,
  getDictationLanguageMode,
  getDictationLanguageModeLabel,
  getDictationLanguagePickerOptions,
  getNextDictationLanguageSelection,
  getSettingsForDictationLanguageSelection,
} from "@/lib/dictationLanguageMode";
import { getLanguageDirection } from "@/lib/utils/rtl";

type OverlayState =
  | "idle"
  | "recording"
  | "silence"
  | "transcribing"
  | "processing"
  | "inserted"
  | "copied"
  | "paste_failed"
  | "mic_failed"
  | "dictionary_learned"
  | "cancelled";

const overlayStates = new Set<OverlayState>([
  "idle",
  "recording",
  "silence",
  "transcribing",
  "processing",
  "inserted",
  "copied",
  "paste_failed",
  "mic_failed",
  "dictionary_learned",
  "cancelled",
]);

const stateLabelKeys: Record<OverlayState, string> = {
  idle: "overlay.states.idle",
  recording: "overlay.states.recording",
  silence: "overlay.states.silence",
  transcribing: "overlay.transcribing",
  processing: "overlay.processing",
  inserted: "overlay.states.inserted",
  copied: "overlay.states.copied",
  paste_failed: "overlay.pasteFailed",
  mic_failed: "overlay.states.micFailed",
  dictionary_learned: "overlay.dictionaryLearned.title",
  cancelled: "overlay.states.cancelled",
};

const getOverlayState = (state: unknown): OverlayState =>
  typeof state === "string" && overlayStates.has(state as OverlayState)
    ? (state as OverlayState)
    : "recording";

const shouldExpandForState = (state: OverlayState): boolean => state !== "idle";

const RecordingOverlay: React.FC = () => {
  const { t } = useTranslation();
  const [isVisible, setIsVisible] = useState(false);
  const [isDocked, setIsDocked] = useState(false);
  const [isDockedExpanded, setIsDockedExpanded] = useState(false);
  const [isLanguagePickerOpen, setIsLanguagePickerOpen] = useState(false);
  const [state, setState] = useState<OverlayState>("recording");
  const [recentlyLearnedEntries, setRecentlyLearnedEntries] = useState<
    DictionaryEntry[]
  >([]);
  const [languageSelection, setLanguageSelection] =
    useState<DictationLanguageSelection>({
      dictationLanguageMode: "auto",
      selectedLanguage: "auto",
      adaptiveLanguageShortlist: ["en", "ar"],
    });
  const [levels, setLevels] = useState<number[]>(Array(16).fill(0));
  const smoothedLevelsRef = useRef<number[]>(Array(16).fill(0));
  const isDockedRef = useRef(false);
  const hasReceivedOverlayEventRef = useRef(false);
  const direction = getLanguageDirection(i18n.language);

  const setDockedMode = (enabled: boolean) => {
    isDockedRef.current = enabled;
    setIsDocked(enabled);
  };

  const getDockedPillSetting = async () => {
    const result = await commands.getAppSettings();
    return result.status === "ok" && result.data.docked_pill_enabled === true;
  };

  const shouldShowDockedFeedback = async () =>
    isDockedRef.current || (await getDockedPillSetting());

  const refreshLanguageMode = async () => {
    const result = await commands.getAppSettings();
    if (result.status !== "ok") return;

    const dictationLanguageMode = getDictationLanguageMode({
      dictationLanguageMode: result.data.dictation_language_mode,
      selectedLanguage: result.data.selected_language,
      adaptiveLanguageShortlist: result.data.adaptive_language_shortlist,
    });

    setLanguageSelection(
      getSettingsForDictationLanguageSelection({
        dictationLanguageMode,
        selectedLanguage: result.data.selected_language ?? "auto",
        adaptiveLanguageShortlist:
          result.data.adaptive_language_shortlist ?? [],
      }),
    );
  };

  useEffect(() => {
    const setupEventListeners = async () => {
      // Listen for show-overlay event from Rust
      const unlistenShow = await listen("show-overlay", async (event) => {
        hasReceivedOverlayEventRef.current = true;
        // Sync language from settings each time overlay is shown
        await syncLanguageFromSettings();
        await refreshLanguageMode();
        const overlayState = getOverlayState(event.payload);
        const dockedEnabled = await getDockedPillSetting();
        setDockedMode(dockedEnabled);
        setIsDockedExpanded(
          dockedEnabled ? shouldExpandForState(overlayState) : false,
        );
        setIsLanguagePickerOpen(false);
        setRecentlyLearnedEntries([]);
        setState(overlayState);
        setIsVisible(true);
      });

      const unlistenShowDocked = await listen(
        "show-docked-overlay",
        async () => {
          hasReceivedOverlayEventRef.current = true;
          await syncLanguageFromSettings();
          await refreshLanguageMode();
          setDockedMode(true);
          setIsDockedExpanded(false);
          setIsLanguagePickerOpen(false);
          setState("idle");
          setIsVisible(true);
        },
      );

      const unlistenPasteError = await listen("paste-error", async () => {
        if (!(await shouldShowDockedFeedback())) return;

        setDockedMode(true);
        setIsDockedExpanded(true);
        setIsLanguagePickerOpen(false);
        setState("paste_failed");
        setIsVisible(true);
      });

      const unlistenLanguageGuardBlocked = await listen(
        "language-guard-blocked",
        async () => {
          if (!(await shouldShowDockedFeedback())) return;

          setDockedMode(true);
          setIsDockedExpanded(true);
          setIsLanguagePickerOpen(false);
          setState("copied");
          setIsVisible(true);
        },
      );

      const unlistenDictionaryLearned = await listen<DictionaryEntry[]>(
        "dictionary-entries-learned",
        async (event) => {
          if (event.payload.length === 0) return;
          if (!(await shouldShowDockedFeedback())) return;

          setRecentlyLearnedEntries(event.payload);
          setDockedMode(true);
          setIsDockedExpanded(true);
          setIsLanguagePickerOpen(false);
          setState("dictionary_learned");
          setIsVisible(true);
        },
      );

      const unlistenStateChanged = await listen(
        "overlay-state-changed",
        (event) => {
          const overlayState = getOverlayState(event.payload);
          setIsLanguagePickerOpen(false);
          setState(overlayState);

          if (isDockedRef.current) {
            setIsDockedExpanded(shouldExpandForState(overlayState));
            setIsVisible(true);
          }
        },
      );

      // Listen for hide-overlay event from Rust
      const unlistenHide = await listen("hide-overlay", () => {
        setIsVisible(false);
        setIsDockedExpanded(false);
        setIsLanguagePickerOpen(false);
      });

      // Listen for mic-level updates
      const unlistenLevel = await listen<number[]>("mic-level", (event) => {
        const newLevels = event.payload as number[];

        // Apply smoothing to reduce jitter
        const smoothed = smoothedLevelsRef.current.map((prev, i) => {
          const target = newLevels[i] || 0;
          return prev * 0.7 + target * 0.3; // Smooth transition
        });

        smoothedLevelsRef.current = smoothed;
        setLevels(smoothed.slice(0, 9));
      });

      const dockedEnabled = await getDockedPillSetting();
      if (dockedEnabled && !hasReceivedOverlayEventRef.current) {
        await syncLanguageFromSettings();
        await refreshLanguageMode();
        setDockedMode(true);
        setIsDockedExpanded(false);
        setIsLanguagePickerOpen(false);
        setState("idle");
        setIsVisible(true);
      }

      // Cleanup function
      return () => {
        unlistenShow();
        unlistenShowDocked();
        unlistenPasteError();
        unlistenLanguageGuardBlocked();
        unlistenDictionaryLearned();
        unlistenStateChanged();
        unlistenHide();
        unlistenLevel();
      };
    };

    setupEventListeners();
  }, []);

  const handleLanguageModeClick = async () => {
    if (isDocked || isLanguagePickerOpen) {
      setIsLanguagePickerOpen((current) => !current);
      return;
    }

    const previousSelection = languageSelection;
    const nextSelection = getSettingsForDictationLanguageSelection(
      getNextDictationLanguageSelection(languageSelection),
    );
    await persistLanguageSelection(nextSelection, previousSelection);
  };

  const persistLanguageSelection = async (
    selection: DictationLanguageSelection,
    previousSelection = languageSelection,
  ) => {
    const nextSelection = getSettingsForDictationLanguageSelection(selection);
    setLanguageSelection(nextSelection);
    setIsLanguagePickerOpen(false);
    const result = await commands.changeDictationLanguageModeSetting(
      nextSelection.dictationLanguageMode,
      nextSelection.selectedLanguage,
      nextSelection.adaptiveLanguageShortlist,
    );
    if (result.status !== "ok") {
      setLanguageSelection(previousSelection);
    }
  };

  const getIcon = () => {
    if (state === "recording") {
      return <MicrophoneIcon />;
    } else {
      return <TranscriptionIcon />;
    }
  };

  const showMainWindow = async () => {
    await commands.showMainWindowCommand();
  };

  const openDictionarySettings = async () => {
    await commands.showMainWindowCommand();
    await emit("open-dictionary-settings");
  };

  const handleUndoLearned = async () => {
    if (recentlyLearnedEntries.length === 0) return;

    const result = await commands.undoDictionaryEntries(
      recentlyLearnedEntries.map((entry) => entry.id),
    );
    if (result.status === "ok") {
      setRecentlyLearnedEntries([]);
      setState("recording");
    }
  };

  const isDockedCollapsed = isDocked && !isDockedExpanded;
  const learnedPhrases = recentlyLearnedEntries
    .map((entry) => entry.phrase)
    .join(", ");

  const renderActionButton = (
    label: string,
    onClick: () => unknown | Promise<unknown>,
    icon: React.ReactNode,
  ) => (
    <button
      type="button"
      className="overlay-action-button"
      onClick={() => {
        void onClick();
      }}
      aria-label={label}
      title={label}
    >
      {icon}
    </button>
  );

  const renderRecordingBars = () => (
    <div className="bars-container">
      {levels.map((v, i) => (
        <div
          key={i}
          className="bar"
          style={{
            height: `${Math.min(20, 4 + Math.pow(v, 0.7) * 16)}px`,
            transition: "height 60ms ease-out, opacity 120ms ease-out",
            opacity: Math.max(0.2, v * 1.7),
          }}
        />
      ))}
    </div>
  );

  const renderLanguagePicker = () => (
    <div
      className="language-picker-options"
      role="menu"
      aria-label={t("overlay.languageMode.title")}
    >
      {getDictationLanguagePickerOptions(languageSelection).map((option) => {
        const isSelected =
          option.selection.dictationLanguageMode ===
            languageSelection.dictationLanguageMode &&
          option.selection.selectedLanguage ===
            languageSelection.selectedLanguage;

        return (
          <button
            key={`${option.selection.dictationLanguageMode}-${option.selection.selectedLanguage}-${option.label}`}
            type="button"
            className="language-picker-option"
            role="menuitemradio"
            aria-checked={isSelected}
            onClick={() => {
              void persistLanguageSelection(option.selection);
            }}
          >
            {option.label}
          </button>
        );
      })}
    </div>
  );

  const renderStateStatus = () => (
    <>
      <div className="docked-status" role="status" aria-live="polite">
        <span className="docked-status-title">{t(stateLabelKeys[state])}</span>
      </div>
      <div className="docked-actions">
        {(state === "copied" || state === "paste_failed") &&
          renderActionButton(
            t("overlay.actions.pasteLastTranscript"),
            commands.pasteLastTranscript,
            <ClipboardPaste size={14} strokeWidth={2.25} />,
          )}
        {state !== "mic_failed" &&
          renderActionButton(
            t("overlay.actions.copyLastTranscript"),
            commands.copyLastTranscript,
            <ClipboardCopy size={14} strokeWidth={2.25} />,
          )}
        {renderActionButton(
          t("overlay.actions.openSettings"),
          showMainWindow,
          <Settings size={14} strokeWidth={2.25} />,
        )}
        {(state === "silence" ||
          state === "transcribing" ||
          state === "processing" ||
          state === "mic_failed") &&
          renderActionButton(
            t("overlay.actions.cancel"),
            commands.cancelOperation,
            <CancelIcon />,
          )}
      </div>
    </>
  );

  const renderDockedExpandedContent = () => {
    if (isLanguagePickerOpen) {
      return renderLanguagePicker();
    }

    if (state === "dictionary_learned" && recentlyLearnedEntries.length > 0) {
      return (
        <>
          <div
            className="docked-status"
            role="status"
            aria-live="polite"
            title={learnedPhrases}
          >
            <span className="docked-status-title">
              {t("overlay.dictionaryLearned.title")}
            </span>
            <span className="docked-status-detail">{learnedPhrases}</span>
          </div>
          <div className="docked-actions">
            {renderActionButton(
              t("overlay.actions.reviewDictionary"),
              openDictionarySettings,
              <BookOpen size={14} strokeWidth={2.25} />,
            )}
            {renderActionButton(
              t("overlay.actions.undoLearnedWord"),
              handleUndoLearned,
              <Undo2 size={14} strokeWidth={2.25} />,
            )}
          </div>
        </>
      );
    }

    if (state === "paste_failed") {
      return (
        <>
          <div className="docked-status" role="status" aria-live="polite">
            <span className="docked-status-title">
              {t("overlay.pasteFailed")}
            </span>
          </div>
          <div className="docked-actions">
            {renderActionButton(
              t("overlay.actions.retryPaste"),
              commands.pasteLastTranscript,
              <RotateCcw size={14} strokeWidth={2.25} />,
            )}
            {renderActionButton(
              t("overlay.actions.copyLastTranscript"),
              commands.copyLastTranscript,
              <ClipboardCopy size={14} strokeWidth={2.25} />,
            )}
            {renderActionButton(
              t("overlay.actions.openSettings"),
              showMainWindow,
              <Settings size={14} strokeWidth={2.25} />,
            )}
          </div>
        </>
      );
    }

    if (state === "recording") {
      return (
        <>
          <button
            type="button"
            className="language-mode-chip"
            onClick={handleLanguageModeClick}
            title={t("overlay.languageMode.title")}
            aria-label={t("overlay.languageMode.change")}
          >
            {getDictationLanguageModeLabel(languageSelection)}
          </button>
          {renderRecordingBars()}
          {renderActionButton(
            t("overlay.actions.cancel"),
            commands.cancelOperation,
            <CancelIcon />,
          )}
        </>
      );
    }

    if (state !== "idle") {
      return renderStateStatus();
    }

    return (
      <>
        <button
          type="button"
          className="language-mode-chip"
          onClick={handleLanguageModeClick}
          title={t("overlay.languageMode.title")}
          aria-label={t("overlay.languageMode.change")}
        >
          {getDictationLanguageModeLabel(languageSelection)}
        </button>
        <div className="docked-actions">
          {renderActionButton(
            t("overlay.actions.copyLastTranscript"),
            commands.copyLastTranscript,
            <ClipboardCopy size={14} strokeWidth={2.25} />,
          )}
          {renderActionButton(
            t("overlay.actions.pasteLastTranscript"),
            commands.pasteLastTranscript,
            <ClipboardPaste size={14} strokeWidth={2.25} />,
          )}
          {renderActionButton(
            t("overlay.actions.reviewDictionary"),
            openDictionarySettings,
            <BookOpen size={14} strokeWidth={2.25} />,
          )}
          {renderActionButton(
            t("overlay.actions.openSettings"),
            showMainWindow,
            <Settings size={14} strokeWidth={2.25} />,
          )}
        </div>
      </>
    );
  };

  return (
    <div
      dir={direction}
      data-testid="recording-overlay"
      className={`recording-overlay ${isVisible ? "fade-in" : ""} ${
        isDocked
          ? isDockedExpanded
            ? "docked-expanded"
            : "docked-collapsed"
          : ""
      }`}
      onMouseEnter={() => {
        if (isDockedCollapsed) {
          setIsDockedExpanded(true);
        }
      }}
      onMouseLeave={() => {
        if (isDocked && state === "idle" && !isLanguagePickerOpen) {
          setIsDockedExpanded(false);
        }
      }}
    >
      {isDockedCollapsed ? (
        <button
          type="button"
          className="docked-pill-handle"
          onClick={() => setIsDockedExpanded(true)}
          aria-label={t("overlay.docked.expand")}
          title={t("overlay.docked.expand")}
        >
          <span className="docked-waveform" aria-hidden="true">
            {levels.slice(0, 5).map((v, i) => (
              <span
                key={i}
                className="docked-waveform-bar"
                style={{
                  height: `${Math.min(16, 4 + Math.pow(v, 0.7) * 12)}px`,
                  opacity: Math.max(0.35, v * 1.6),
                }}
              />
            ))}
          </span>
          <span className="docked-dot" aria-hidden="true" />
        </button>
      ) : isDocked ? (
        renderDockedExpandedContent()
      ) : isLanguagePickerOpen ? (
        renderLanguagePicker()
      ) : (
        <>
          <div className="overlay-left">{getIcon()}</div>

          <div className="overlay-middle">
            {state === "recording" && renderRecordingBars()}
            {state === "transcribing" && (
              <div className="transcribing-text">
                {t("overlay.transcribing")}
              </div>
            )}
            {state === "processing" && (
              <div className="transcribing-text">{t("overlay.processing")}</div>
            )}
            {state !== "recording" &&
              state !== "transcribing" &&
              state !== "processing" && (
                <div className="transcribing-text">
                  {t(stateLabelKeys[state])}
                </div>
              )}
          </div>

          <div className="overlay-right">
            <button
              type="button"
              className="language-mode-chip"
              onClick={handleLanguageModeClick}
              title={t("overlay.languageMode.title")}
              aria-label={t("overlay.languageMode.change")}
            >
              {getDictationLanguageModeLabel(languageSelection)}
            </button>
            {state === "recording" && (
              <div
                className="cancel-button"
                onClick={() => {
                  commands.cancelOperation();
                }}
              >
                <CancelIcon />
              </div>
            )}
          </div>
        </>
      )}
    </div>
  );
};

export default RecordingOverlay;
