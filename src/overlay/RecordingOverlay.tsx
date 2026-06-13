import { listen } from "@tauri-apps/api/event";
import React, { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  MicrophoneIcon,
  TranscriptionIcon,
  CancelIcon,
} from "../components/icons";
import "./RecordingOverlay.css";
import { commands } from "@/bindings";
import i18n, { syncLanguageFromSettings } from "@/i18n";
import {
  type DictationLanguageSelection,
  getDictationLanguageMode,
  getDictationLanguageModeLabel,
  getNextDictationLanguageSelection,
  getSettingsForDictationLanguageSelection,
} from "@/lib/dictationLanguageMode";
import { getLanguageDirection } from "@/lib/utils/rtl";

type OverlayState = "recording" | "transcribing" | "processing";

const RecordingOverlay: React.FC = () => {
  const { t } = useTranslation();
  const [isVisible, setIsVisible] = useState(false);
  const [isDocked, setIsDocked] = useState(false);
  const [isDockedExpanded, setIsDockedExpanded] = useState(false);
  const [state, setState] = useState<OverlayState>("recording");
  const [languageSelection, setLanguageSelection] =
    useState<DictationLanguageSelection>({
      dictationLanguageMode: "auto",
      selectedLanguage: "auto",
      adaptiveLanguageShortlist: ["en", "ar"],
    });
  const [levels, setLevels] = useState<number[]>(Array(16).fill(0));
  const smoothedLevelsRef = useRef<number[]>(Array(16).fill(0));
  const direction = getLanguageDirection(i18n.language);

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
        // Sync language from settings each time overlay is shown
        await syncLanguageFromSettings();
        await refreshLanguageMode();
        const overlayState = event.payload as OverlayState;
        setIsDocked(false);
        setIsDockedExpanded(false);
        setState(overlayState);
        setIsVisible(true);
      });

      const unlistenShowDocked = await listen(
        "show-docked-overlay",
        async () => {
          await syncLanguageFromSettings();
          await refreshLanguageMode();
          setIsDocked(true);
          setIsDockedExpanded(false);
          setState("recording");
          setIsVisible(true);
        },
      );

      // Listen for hide-overlay event from Rust
      const unlistenHide = await listen("hide-overlay", () => {
        setIsVisible(false);
        setIsDockedExpanded(false);
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

      // Cleanup function
      return () => {
        unlistenShow();
        unlistenShowDocked();
        unlistenHide();
        unlistenLevel();
      };
    };

    setupEventListeners();
  }, []);

  const handleLanguageModeClick = async () => {
    const previousSelection = languageSelection;
    const nextSelection = getSettingsForDictationLanguageSelection(
      getNextDictationLanguageSelection(languageSelection),
    );
    setLanguageSelection(nextSelection);
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

  const isDockedCollapsed = isDocked && !isDockedExpanded;

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
        if (isDocked) {
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
      ) : (
        <>
          <div className="overlay-left">{getIcon()}</div>

          <div className="overlay-middle">
            {state === "recording" && (
              <div className="bars-container">
                {levels.map((v, i) => (
                  <div
                    key={i}
                    className="bar"
                    style={{
                      height: `${Math.min(20, 4 + Math.pow(v, 0.7) * 16)}px`, // Cap at 20px max height
                      transition:
                        "height 60ms ease-out, opacity 120ms ease-out",
                      opacity: Math.max(0.2, v * 1.7), // Minimum opacity for visibility
                    }}
                  />
                ))}
              </div>
            )}
            {state === "transcribing" && (
              <div className="transcribing-text">
                {t("overlay.transcribing")}
              </div>
            )}
            {state === "processing" && (
              <div className="transcribing-text">{t("overlay.processing")}</div>
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
