import {
  type FormEvent,
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import {
  ArrowLeft,
  BookOpen,
  Check,
  ChevronRight,
  Copy,
  Cpu,
  ExternalLink,
  FileText,
  History,
  Home,
  Info,
  Languages,
  MapPin,
  Mic,
  MicOff,
  Moon,
  Music,
  Pencil,
  RefreshCw,
  Search,
  Settings,
  Share2,
  SlidersHorizontal,
  Sparkles,
  Star,
  Sun,
  Trash2,
  Volume1,
  X,
} from "lucide-react";
import {
  commands,
  type AudioDevice,
  type DictionaryEntry,
  type HistoryEntry,
  type LLMPrompt,
  type ModelInfo,
  type PostProcessProvider,
  type RecordingRetentionPeriod,
  type SnippetEntry,
  type SoundTheme,
} from "@/bindings";
import { useSettings } from "@/hooks/useSettings";
import {
  changeAppLanguage,
  SUPPORTED_LANGUAGES,
  type SupportedLanguageCode,
} from "@/i18n";
import { getDisplayVersion } from "@/lib/appVersion";
import { useDictionaryStore } from "@/stores/dictionaryStore";
import { useModelStore } from "@/stores/modelStore";
import { useSettingsStore } from "@/stores/settingsStore";
import { useSnippetsStore } from "@/stores/snippetsStore";
import { formatDateTime } from "@/utils/dateFormat";
import "./AndroidApp.css";

type AndroidTab = "home" | "history" | "models" | "settings";
type AndroidTheme = "system" | "light" | "dark";
type LibrarySection = "dictionary" | "snippets";
type SettingsSubscreen =
  | { type: "library"; section: LibrarySection }
  | { type: "postProcessing" }
  | { type: "about" };
type SettingsSheet =
  | "microphone"
  | "output"
  | "bubblePosition"
  | "appLanguage"
  | "historyLimit"
  | "recordingRetention"
  | "soundTheme";
type PromptEditorState =
  | { mode: "create" }
  | { mode: "edit"; prompt: LLMPrompt };
type AndroidBubbleCorner =
  | "top-left"
  | "top-right"
  | "bottom-left"
  | "bottom-right";
type AndroidRetentionPeriod =
  | "never"
  | "preserve_limit"
  | "days3"
  | "weeks2"
  | "months3";
type AndroidSpeechModelStatus =
  | "unknown"
  | "ready"
  | "missing"
  | "pending"
  | "downloading"
  | "error"
  | "unsupported";

type AndroidPermissionSnapshot = {
  microphone: boolean;
  overlay: boolean;
  accessibility: boolean;
  bubbleRunning: boolean;
  bubbleVisible: boolean;
  speechRecognizerAvailable: boolean;
  onDeviceSpeechRecognizerAvailable: boolean;
  onDeviceSpeechLanguageAvailable: boolean;
  onDeviceSpeechModelStatus: AndroidSpeechModelStatus;
};

declare global {
  interface Window {
    VerbatimAndroid?: {
      permissionSnapshot: () => string;
      nativeTranscriptHistory?: () => string;
      syncTextFormatter?: (snapshot: string) => void;
      requestMicrophone: () => void;
      openOverlaySettings: () => void;
      openAccessibilitySettings: () => void;
      requestSpeechModelDownload: () => void;
      startBubble: () => void;
      stopBubble: () => void;
      bubbleCornerSnapshot?: () => string;
      setBubbleCorner?: (corner: AndroidBubbleCorner) => string;
      openExternalUrl?: (url: string) => boolean;
    };
  }
}

const defaultPermissions: AndroidPermissionSnapshot = {
  microphone: false,
  overlay: false,
  accessibility: false,
  bubbleRunning: false,
  bubbleVisible: false,
  speechRecognizerAvailable: false,
  onDeviceSpeechRecognizerAvailable: false,
  onDeviceSpeechLanguageAvailable: false,
  onDeviceSpeechModelStatus: "unknown",
};

const tabs: Array<{ id: AndroidTab; labelKey: string; icon: typeof Home }> = [
  { id: "home", labelKey: "android.tabs.home", icon: Home },
  { id: "history", labelKey: "android.tabs.history", icon: History },
  { id: "models", labelKey: "android.tabs.models", icon: Cpu },
  { id: "settings", labelKey: "android.tabs.settings", icon: Settings },
];

const ANDROID_EXCLUDED_POST_PROCESS_PROVIDERS = new Set(["apple_intelligence"]);
const DEFAULT_DEVICE_VALUE = "Default";
const VERBATIM_SOURCE_URL = "https://github.com/GalaxyRuler/Verbatim";
const HANDY_SOURCE_URL = "https://github.com/cjpais/Handy";
const androidBubbleCorners: AndroidBubbleCorner[] = [
  "top-left",
  "top-right",
  "bottom-left",
  "bottom-right",
];

const safeBridge = () => window.VerbatimAndroid;

const openAndroidExternalUrl = (url: string) => {
  safeBridge()?.openExternalUrl?.(url);
};

const isAndroidPostProcessProvider = (provider: PostProcessProvider) =>
  !ANDROID_EXCLUDED_POST_PROCESS_PROVIDERS.has(provider.id);

const parsePermissions = (value: string): AndroidPermissionSnapshot => {
  try {
    return { ...defaultPermissions, ...JSON.parse(value) };
  } catch {
    return defaultPermissions;
  }
};

const normalizeBubbleCorner = (
  value: string | null | undefined,
): AndroidBubbleCorner =>
  androidBubbleCorners.includes(value as AndroidBubbleCorner)
    ? (value as AndroidBubbleCorner)
    : "top-right";

const normalizeDeviceSetting = (value: string | null | undefined): string => {
  if (!value || value.toLowerCase() === "default") {
    return DEFAULT_DEVICE_VALUE;
  }

  return value;
};

const normalizeRetentionPeriod = (
  value: string | null | undefined,
): AndroidRetentionPeriod => {
  switch (value) {
    case "days_3":
    case "days3":
      return "days3";
    case "weeks_2":
    case "weeks2":
      return "weeks2";
    case "months_3":
    case "months3":
      return "months3";
    case "preserve_limit":
      return "preserve_limit";
    case "never":
    default:
      return "never";
  }
};

const deviceOptions = (devices: AudioDevice[], defaultLabel: string) => {
  const seen = new Set(["default"]);
  return [
    { value: DEFAULT_DEVICE_VALUE, label: defaultLabel, description: "" },
    ...devices.flatMap((device) => {
      const normalized = device.name.toLowerCase();
      if (seen.has(normalized)) {
        return [];
      }
      seen.add(normalized);
      return [
        {
          value: device.name,
          label: device.name,
          description: device.is_default ? defaultLabel : "",
        },
      ];
    }),
  ];
};

const soundThemeOptions: SoundTheme[] = ["marimba", "pop", "custom"];

const retentionPeriods: AndroidRetentionPeriod[] = [
  "never",
  "preserve_limit",
  "days3",
  "weeks2",
  "months3",
];

const historyEntryFromAndroidSnapshot = (
  entry: Record<string, unknown>,
): HistoryEntry | null => {
  const text = entry.transcription_text;
  if (typeof text !== "string" || text.trim().length === 0) {
    return null;
  }

  const id = Number(entry.id);
  const timestamp = Number(entry.timestamp);
  const fallbackTimestamp = Date.now();
  const normalizedTimestamp = Number.isFinite(timestamp)
    ? timestamp
    : fallbackTimestamp;
  return {
    id: Number.isFinite(id) ? id : normalizedTimestamp,
    file_name: "android-native",
    timestamp: normalizedTimestamp,
    saved: false,
    title: typeof entry.title === "string" ? entry.title : "",
    transcription_text: text,
    post_processed_text:
      typeof entry.post_processed_text === "string" &&
      entry.post_processed_text.trim().length > 0
        ? entry.post_processed_text
        : null,
    post_process_prompt: null,
    post_process_requested: false,
    adaptive_profile_id: null,
    adaptive_profile_name: null,
    adaptive_routing_json: null,
    adaptive_context_json: null,
    adaptive_language_json: null,
    adaptive_insertion_json: null,
    adaptive_parent_entry_id: null,
    transform_action: null,
    transform_original_text: null,
    transform_result_text: null,
    transform_target_language: null,
    transform_provider_id: null,
    transform_model: null,
    transform_recovery_status: null,
  };
};

const historyDisplayText = (entry: HistoryEntry): string =>
  entry.post_processed_text || entry.transcription_text;

const readAndroidNativeHistory = (): HistoryEntry[] => {
  const snapshot = safeBridge()?.nativeTranscriptHistory?.();
  if (!snapshot) {
    return [];
  }

  try {
    const entries = JSON.parse(snapshot);
    if (!Array.isArray(entries)) {
      return [];
    }
    return entries
      .map((entry) =>
        typeof entry === "object" && entry !== null && !Array.isArray(entry)
          ? historyEntryFromAndroidSnapshot(entry as Record<string, unknown>)
          : null,
      )
      .filter((entry): entry is HistoryEntry => entry !== null);
  } catch {
    return [];
  }
};

const Switch = ({
  checked,
  label,
  onClick,
}: {
  checked: boolean;
  label: string;
  onClick?: () => void;
}) => (
  <button
    type="button"
    aria-label={label}
    aria-pressed={checked}
    className={`android-switch ${checked ? "android-switch-on" : ""}`}
    onClick={onClick}
  >
    <span className="android-switch-thumb" />
  </button>
);

const WaveformPreview = () => (
  <div className="android-bubble-preview" aria-hidden="true">
    <Mic size={20} />
    <div className="android-wave">
      <span />
      <span />
      <span />
      <span />
      <span />
    </div>
    <span className="android-stop-dot" />
  </div>
);

const useAndroidTextFormatterSync = () => {
  const dictionaryEntries = useDictionaryStore((store) => store.entries);
  const loadDictionaryEntries = useDictionaryStore(
    (store) => store.loadEntries,
  );
  const snippetEntries = useSnippetsStore((store) => store.entries);
  const loadSnippetEntries = useSnippetsStore((store) => store.loadEntries);

  useEffect(() => {
    if (!safeBridge()?.syncTextFormatter) {
      return;
    }

    void Promise.all([loadDictionaryEntries(), loadSnippetEntries()]).catch(
      () => undefined,
    );
  }, [loadDictionaryEntries, loadSnippetEntries]);

  useEffect(() => {
    const bridge = safeBridge();
    if (!bridge?.syncTextFormatter) {
      return;
    }

    bridge.syncTextFormatter(
      JSON.stringify({
        dictionary_entries: dictionaryEntries.map((entry) => ({
          phrase: entry.phrase,
          replacement_of: entry.replacement_of ?? null,
          priority: entry.priority ?? "normal",
        })),
        snippets: snippetEntries.map((entry) => ({
          trigger: entry.trigger,
          content: entry.content,
        })),
      }),
    );
  }, [dictionaryEntries, snippetEntries]);
};

export default function AndroidApp() {
  const { t, i18n } = useTranslation();
  const [activeTab, setActiveTab] = useState<AndroidTab>("home");
  const [settingsSubscreen, setSettingsSubscreen] =
    useState<SettingsSubscreen | null>(null);
  const [theme, setTheme] = useState<AndroidTheme>(() => {
    const stored = window.localStorage.getItem("verbatim.android.theme");
    return stored === "light" || stored === "dark" || stored === "system"
      ? stored
      : "system";
  });
  const [permissions, setPermissions] =
    useState<AndroidPermissionSnapshot>(defaultPermissions);

  useAndroidTextFormatterSync();

  const refreshPermissions = useCallback(() => {
    const snapshot = safeBridge()?.permissionSnapshot();
    if (snapshot) {
      setPermissions(parsePermissions(snapshot));
    }
  }, []);

  useEffect(() => {
    refreshPermissions();
    const interval = window.setInterval(refreshPermissions, 1200);
    window.addEventListener("focus", refreshPermissions);
    return () => {
      window.clearInterval(interval);
      window.removeEventListener("focus", refreshPermissions);
    };
  }, [refreshPermissions]);

  useEffect(() => {
    window.localStorage.setItem("verbatim.android.theme", theme);
  }, [theme]);

  const allPermissionsReady =
    permissions.microphone &&
    permissions.overlay &&
    permissions.accessibility &&
    permissions.onDeviceSpeechRecognizerAvailable &&
    permissions.onDeviceSpeechLanguageAvailable;

  const activeTabSpec = tabs.find((tab) => tab.id === activeTab) ?? tabs[0];
  const settingsSubscreenTitle = useMemo(() => {
    if (!settingsSubscreen) return null;
    if (settingsSubscreen.type === "postProcessing") {
      return t("settings.postProcessing.title");
    }
    if (settingsSubscreen.type === "about") {
      return t("settings.about.title");
    }
    return t(
      settingsSubscreen.section === "dictionary"
        ? "settings.dictionary.title"
        : "settings.snippets.title",
    );
  }, [settingsSubscreen, t]);
  const title = settingsSubscreenTitle ?? t(activeTabSpec.labelKey);
  const showSettingsBack = activeTab === "settings" && !!settingsSubscreen;

  return (
    <div className={`android-app android-theme-${theme}`}>
      <main className="android-shell">
        <header className="android-top-bar">
          <div className="android-top-title">
            {showSettingsBack && (
              <button
                type="button"
                className="android-icon-button"
                aria-label={t("common.cancel")}
                onClick={() => setSettingsSubscreen(null)}
              >
                <ArrowLeft size={22} />
              </button>
            )}
            <h1>{activeTab === "home" ? t("common.appName") : title}</h1>
          </div>
          <button
            type="button"
            className="android-icon-button"
            aria-label={t("android.actions.toggleTheme")}
            onClick={() =>
              setTheme((current) =>
                current === "system"
                  ? "light"
                  : current === "light"
                    ? "dark"
                    : "system",
              )
            }
          >
            {theme === "dark" ? <Moon size={22} /> : <Sun size={22} />}
          </button>
        </header>

        {activeTab === "models" ? (
          <ModelsTab />
        ) : !allPermissionsReady ? (
          <AndroidOnboarding
            permissions={permissions}
            refreshPermissions={refreshPermissions}
          />
        ) : activeTab === "home" ? (
          <HomeTab
            permissions={permissions}
            goToModels={() => setActiveTab("models")}
            goToHistory={() => setActiveTab("history")}
          />
        ) : activeTab === "history" ? (
          <HistoryTab />
        ) : settingsSubscreen?.type === "library" ? (
          <LibraryTab
            activeSection={settingsSubscreen.section}
            setActiveSection={(section) =>
              setSettingsSubscreen({ type: "library", section })
            }
          />
        ) : settingsSubscreen?.type === "postProcessing" ? (
          <AndroidPostProcessingScreen />
        ) : settingsSubscreen?.type === "about" ? (
          <AndroidAboutScreen />
        ) : (
          <SettingsTab
            theme={theme}
            setTheme={setTheme}
            openLibrary={(section) =>
              setSettingsSubscreen({ type: "library", section })
            }
            openPostProcessing={() =>
              setSettingsSubscreen({ type: "postProcessing" })
            }
            openAbout={() => setSettingsSubscreen({ type: "about" })}
          />
        )}
      </main>

      {!showSettingsBack && (
        <nav className="android-nav" aria-label={t("android.tabs.navigation")}>
          {tabs.map((tab) => {
            const Icon = tab.icon;
            const active = activeTab === tab.id;
            return (
              <button
                type="button"
                key={tab.id}
                className={active ? "android-nav-active" : ""}
                onClick={() => {
                  setSettingsSubscreen(null);
                  setActiveTab(tab.id);
                }}
              >
                <span className="android-nav-icon">
                  <Icon size={22} />
                </span>
                <span>{t(tab.labelKey)}</span>
              </button>
            );
          })}
        </nav>
      )}
    </div>
  );
}

function AndroidOnboarding({
  permissions,
  refreshPermissions,
}: {
  permissions: AndroidPermissionSnapshot;
  refreshPermissions: () => void;
}) {
  const { t } = useTranslation();
  const bridge = safeBridge();
  const speechPackCalloutKey = (() => {
    switch (permissions.onDeviceSpeechModelStatus) {
      case "ready":
        return "android.onboarding.speechPack.readyCallout";
      case "pending":
        return "android.onboarding.speechPack.pendingCallout";
      case "downloading":
        return "android.onboarding.speechPack.downloadingCallout";
      case "error":
        return "android.onboarding.speechPack.errorCallout";
      case "unsupported":
        return "android.onboarding.speechPack.unsupportedCallout";
      default:
        return "android.onboarding.speechPack.callout";
    }
  })();
  const steps = [
    {
      ready: permissions.microphone,
      title: t("android.onboarding.microphone.title"),
      description: t("android.onboarding.microphone.description"),
      action: t("android.onboarding.microphone.action"),
      onClick: () => bridge?.requestMicrophone(),
      callout: null,
    },
    {
      ready: permissions.overlay,
      title: t("android.onboarding.overlay.title"),
      description: t("android.onboarding.overlay.description"),
      action: t("android.onboarding.overlay.action"),
      onClick: () => bridge?.openOverlaySettings(),
      callout: t("android.onboarding.overlay.callout"),
    },
    {
      ready: permissions.accessibility,
      title: t("android.onboarding.accessibility.title"),
      description: t("android.onboarding.accessibility.description"),
      action: t("android.onboarding.accessibility.action"),
      onClick: () => bridge?.openAccessibilitySettings(),
      callout: t("android.onboarding.accessibility.callout"),
    },
    {
      ready: permissions.onDeviceSpeechRecognizerAvailable,
      title: t("android.onboarding.speech.title"),
      description: t("android.onboarding.speech.description"),
      action: null,
      onClick: null,
      callout: permissions.onDeviceSpeechRecognizerAvailable
        ? t("android.onboarding.speech.readyCallout")
        : t("android.onboarding.speech.missingCallout"),
    },
    {
      ready: permissions.onDeviceSpeechLanguageAvailable,
      title: t("android.onboarding.speechPack.title"),
      description: t("android.onboarding.speechPack.description"),
      action: t("android.onboarding.speechPack.action"),
      onClick: () => bridge?.requestSpeechModelDownload(),
      callout: t(speechPackCalloutKey),
    },
  ];
  const currentStepIndex = steps.findIndex((step) => !step.ready);
  const currentStep =
    steps[currentStepIndex >= 0 ? currentStepIndex : steps.length - 1];

  return (
    <section className="android-section">
      <div className="android-hero android-panel">
        <div>
          <h2>{t("android.onboarding.title")}</h2>
          <p className="android-muted">{t("android.onboarding.subtitle")}</p>
        </div>
        <WaveformPreview />
      </div>

      <div className="android-permission-list">
        <div className="android-permission-row">
          <div className="android-permission-copy">
            <span className="android-step-label">
              {t("android.onboarding.step", {
                current: currentStepIndex + 1,
                total: steps.length,
              })}
            </span>
            <h3>{currentStep.title}</h3>
            <p className="android-muted">{currentStep.description}</p>
            {currentStep.callout && (
              <div
                className={`android-callout ${
                  currentStep.ready
                    ? "android-callout-trust"
                    : "android-callout-warning"
                }`}
              >
                {currentStep.callout}
              </div>
            )}
          </div>
          {currentStep.ready ? (
            <Check aria-label={t("android.permissions.granted")} size={24} />
          ) : currentStep.action && currentStep.onClick ? (
            <button
              type="button"
              className="android-action android-primary-action"
              onClick={() => {
                currentStep.onClick();
                window.setTimeout(refreshPermissions, 600);
              }}
            >
              {currentStep.action}
            </button>
          ) : (
            <span className="android-status-pill android-status-warning">
              {t("android.permissions.unavailable")}
            </span>
          )}
        </div>
      </div>
    </section>
  );
}

function HomeTab({
  permissions,
  goToModels,
  goToHistory,
}: {
  permissions: AndroidPermissionSnapshot;
  goToModels: () => void;
  goToHistory: () => void;
}) {
  const { t, i18n } = useTranslation();
  const { settings } = useSettings();
  const { models, currentModel } = useModelStore();
  const [lastEntry, setLastEntry] = useState<HistoryEntry | null>(null);
  const activeModel = models.find((model) => model.id === currentModel);
  const selectedLanguage = settings?.selected_language || t("common.default");

  const loadLastEntry = useCallback(() => {
    const nativeEntries = readAndroidNativeHistory();
    if (nativeEntries.length > 0) {
      setLastEntry(nativeEntries[0]);
      return;
    }

    commands.getHistoryEntries(null, 1).then((result) => {
      if (result.status === "ok") {
        setLastEntry(result.data.entries[0] ?? null);
      }
    });
  }, []);

  useEffect(() => {
    loadLastEntry();
    const interval = window.setInterval(loadLastEntry, 1500);
    return () => window.clearInterval(interval);
  }, [loadLastEntry]);

  const bubbleReady =
    permissions.microphone &&
    permissions.overlay &&
    permissions.accessibility &&
    permissions.onDeviceSpeechRecognizerAvailable &&
    permissions.onDeviceSpeechLanguageAvailable;

  return (
    <>
      <section className="android-hero android-panel">
        <div className="android-hero-row">
          <div>
            <h2>
              {bubbleReady
                ? t("android.home.bubble.activeTitle")
                : t("android.home.bubble.inactiveTitle")}
            </h2>
            <p className="android-muted">
              {bubbleReady
                ? t("android.home.bubble.activeSubtitle")
                : t("android.home.bubble.inactiveSubtitle")}
            </p>
          </div>
          {bubbleReady ? <Mic size={28} /> : <MicOff size={28} />}
        </div>
        <WaveformPreview />
        <div className="android-hero-row">
          <span>{t("android.home.bubble.statusLabel")}</span>
          <span
            className={`android-status-pill ${
              permissions.bubbleVisible
                ? "android-status-trust"
                : "android-status-warning"
            }`}
          >
            {permissions.bubbleVisible
              ? t("android.home.bubble.visible")
              : t("android.home.bubble.waiting")}
          </span>
        </div>
      </section>

      <div className="android-chips">
        <button type="button" className="android-chip" onClick={goToModels}>
          <Cpu size={18} />
          <span>{activeModel?.name ?? t("android.home.modelMissing")}</span>
        </button>
        <button type="button" className="android-chip">
          <SlidersHorizontal size={18} />
          <span>{selectedLanguage}</span>
        </button>
      </div>

      <section className="android-card android-panel">
        <div className="android-card-header">
          <div>
            <h2>{t("android.home.lastTranscript.title")}</h2>
            <p className="android-muted">
              {lastEntry
                ? formatDateTime(String(lastEntry.timestamp), i18n.language)
                : t("android.home.lastTranscript.emptyTime")}
            </p>
          </div>
          <button
            type="button"
            className="android-action"
            onClick={goToHistory}
          >
            {t("android.home.lastTranscript.seeAll")}
          </button>
        </div>
        <p className="android-transcript">
          {lastEntry
            ? historyDisplayText(lastEntry)
            : t("android.home.lastTranscript.empty")}
        </p>
        {lastEntry && (
          <div className="android-actions">
            <button
              type="button"
              className="android-action"
              onClick={() =>
                navigator.clipboard.writeText(historyDisplayText(lastEntry))
              }
            >
              <Copy size={17} />
              <span>{t("android.actions.copy")}</span>
            </button>
            <button type="button" className="android-action">
              <Share2 size={17} />
              <span>{t("android.actions.share")}</span>
            </button>
          </div>
        )}
      </section>
    </>
  );
}

function HistoryTab() {
  const { t, i18n } = useTranslation();
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [expandedId, setExpandedId] = useState<number | null>(null);
  const [search, setSearch] = useState("");

  const loadEntries = useCallback(() => {
    const nativeEntries = readAndroidNativeHistory();
    if (nativeEntries.length > 0) {
      setEntries(nativeEntries);
      return;
    }

    commands.getHistoryEntries(null, 30).then((result) => {
      if (result.status === "ok") {
        setEntries(result.data.entries);
      }
    });
  }, []);

  useEffect(() => {
    loadEntries();
  }, [loadEntries]);

  const filteredEntries = entries.filter((entry) =>
    historyDisplayText(entry).toLowerCase().includes(search.toLowerCase()),
  );

  return (
    <section className="android-section">
      <input
        className="android-search"
        value={search}
        onChange={(event) => setSearch(event.target.value)}
        placeholder={t("android.history.search")}
        aria-label={t("android.history.search")}
      />

      <div className="android-list" style={{ marginTop: 14 }}>
        {filteredEntries.length === 0 ? (
          <div className="android-card android-panel">
            <p className="android-muted">{t("settings.history.empty")}</p>
          </div>
        ) : (
          filteredEntries.map((entry) => {
            const expanded = expandedId === entry.id;
            return (
              <article
                key={entry.id}
                className="android-history-card"
                onClick={() => setExpandedId(expanded ? null : entry.id)}
              >
                <div className="android-card-header">
                  <div>
                    <h2>
                      {formatDateTime(String(entry.timestamp), i18n.language)}
                    </h2>
                    <p className="android-muted">{entry.title}</p>
                  </div>
                  <Star
                    size={18}
                    fill={entry.saved ? "currentColor" : "none"}
                  />
                </div>
                <p className="android-transcript">
                  {historyDisplayText(entry)}
                </p>
                {expanded && (
                  <div className="android-actions">
                    <button
                      type="button"
                      className="android-action"
                      onClick={(event) => {
                        event.stopPropagation();
                        navigator.clipboard.writeText(
                          historyDisplayText(entry),
                        );
                      }}
                    >
                      <Copy size={17} />
                      <span>{t("android.actions.copy")}</span>
                    </button>
                    <button type="button" className="android-action">
                      <BookOpen size={17} />
                      <span>{t("settings.history.learnCorrection")}</span>
                    </button>
                    <button type="button" className="android-action">
                      <Trash2 size={17} />
                      <span>{t("common.delete")}</span>
                    </button>
                  </div>
                )}
              </article>
            );
          })
        )}
      </div>
    </section>
  );
}

function LibraryTab({
  activeSection,
  setActiveSection,
}: {
  activeSection: LibrarySection;
  setActiveSection: (section: LibrarySection) => void;
}) {
  const { t } = useTranslation();
  const [search, setSearch] = useState("");

  useEffect(() => {
    setSearch("");
  }, [activeSection]);

  return (
    <>
      <section className="android-section">
        <div
          className="android-segments android-library-tabs"
          role="tablist"
          aria-label={t("settings.dictionary.title")}
        >
          <button
            type="button"
            role="tab"
            aria-selected={activeSection === "dictionary"}
            className={`android-segment ${
              activeSection === "dictionary" ? "android-segment-active" : ""
            }`}
            onClick={() => setActiveSection("dictionary")}
          >
            {t("settings.dictionary.title")}
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={activeSection === "snippets"}
            className={`android-segment ${
              activeSection === "snippets" ? "android-segment-active" : ""
            }`}
            onClick={() => setActiveSection("snippets")}
          >
            {t("settings.snippets.title")}
          </button>
        </div>
      </section>

      <section className="android-section">
        <input
          className="android-search"
          value={search}
          onChange={(event) => setSearch(event.target.value)}
          placeholder={t(
            activeSection === "dictionary"
              ? "settings.dictionary.search"
              : "settings.snippets.search",
          )}
          aria-label={t(
            activeSection === "dictionary"
              ? "settings.dictionary.search"
              : "settings.snippets.search",
          )}
        />
      </section>

      {activeSection === "dictionary" ? (
        <AndroidDictionaryPanel search={search} />
      ) : (
        <AndroidSnippetsPanel search={search} />
      )}
    </>
  );
}

function AndroidDictionaryPanel({ search }: { search: string }) {
  const { t } = useTranslation();
  const {
    entries,
    isLoading,
    updatingIds,
    loadEntries,
    addEntry,
    updateEntry,
    deleteEntry,
  } = useDictionaryStore();
  const [phrase, setPhrase] = useState("");
  const [replacementOf, setReplacementOf] = useState("");
  const [editingId, setEditingId] = useState<string | null>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    loadEntries().catch(() => {
      setError(t("settings.dictionary.errors.load"));
    });
  }, [loadEntries, t]);

  const filteredEntries = useMemo(() => {
    const query = search.trim().toLowerCase();
    if (!query) return entries;

    return entries.filter((entry) =>
      [entry.phrase, entry.replacement_of ?? ""].some((value) =>
        value.toLowerCase().includes(query),
      ),
    );
  }, [entries, search]);

  const resetForm = () => {
    setPhrase("");
    setReplacementOf("");
    setEditingId(null);
  };

  const handleSubmit = async (event: FormEvent) => {
    event.preventDefault();
    const nextPhrase = phrase.trim();
    if (!nextPhrase || nextPhrase.length > 120) return;

    try {
      setError("");
      const input = {
        phrase: nextPhrase,
        replacement_of: replacementOf.trim() || null,
      };
      if (editingId) {
        await updateEntry(editingId, input);
      } else {
        await addEntry(input);
      }
      resetForm();
    } catch {
      setError(
        t(
          editingId
            ? "settings.dictionary.errors.update"
            : "settings.dictionary.errors.add",
        ),
      );
    }
  };

  const handleEdit = (entry: DictionaryEntry) => {
    setPhrase(entry.phrase);
    setReplacementOf(entry.replacement_of ?? "");
    setEditingId(entry.id);
    setError("");
  };

  const handleDelete = async (entry: DictionaryEntry) => {
    try {
      setError("");
      await deleteEntry(entry.id);
      if (editingId === entry.id) {
        resetForm();
      }
    } catch {
      setError(t("settings.dictionary.errors.delete"));
    }
  };

  const handleToggleStar = async (entry: DictionaryEntry) => {
    try {
      setError("");
      await updateEntry(entry.id, {
        priority: entry.priority === "starred" ? "normal" : "starred",
      });
    } catch {
      setError(t("settings.dictionary.errors.update"));
    }
  };

  return (
    <section className="android-section">
      <div className="android-section-header">
        <h2>{t("settings.dictionary.title")}</h2>
        <span className="android-badge">
          {t("settings.dictionary.counts.total", { count: entries.length })}
        </span>
      </div>

      <form className="android-library-form" onSubmit={handleSubmit}>
        <label className="android-field">
          <span>{t("settings.dictionary.phrase")}</span>
          <input
            value={phrase}
            onChange={(event) => setPhrase(event.target.value)}
            maxLength={120}
          />
        </label>
        <label className="android-field">
          <span>{t("settings.dictionary.replacementOf")}</span>
          <input
            value={replacementOf}
            onChange={(event) => setReplacementOf(event.target.value)}
            maxLength={120}
            placeholder={t("settings.dictionary.replacementPlaceholder")}
          />
        </label>
        <div className="android-actions">
          <button
            type="submit"
            className="android-action android-primary-action"
            disabled={phrase.trim().length === 0}
          >
            <Check size={17} />
            <span>
              {editingId
                ? t("settings.dictionary.save")
                : t("settings.dictionary.add")}
            </span>
          </button>
          {editingId && (
            <button
              type="button"
              className="android-action"
              onClick={resetForm}
            >
              <X size={17} />
              <span>{t("settings.dictionary.cancel")}</span>
            </button>
          )}
        </div>
      </form>

      {error && (
        <p className="android-error-text" role="alert">
          {error}
        </p>
      )}

      <AndroidDictionaryList
        entries={filteredEntries}
        totalCount={entries.length}
        isLoading={isLoading}
        updatingIds={updatingIds}
        onEdit={handleEdit}
        onDelete={handleDelete}
        onToggleStar={handleToggleStar}
      />
    </section>
  );
}

function AndroidDictionaryList({
  entries,
  totalCount,
  isLoading,
  updatingIds,
  onEdit,
  onDelete,
  onToggleStar,
}: {
  entries: DictionaryEntry[];
  totalCount: number;
  isLoading: boolean;
  updatingIds: Set<string>;
  onEdit: (entry: DictionaryEntry) => void;
  onDelete: (entry: DictionaryEntry) => void;
  onToggleStar: (entry: DictionaryEntry) => void;
}) {
  const { t } = useTranslation();

  if (isLoading) {
    return (
      <div className="android-card android-panel">
        <p className="android-muted">{t("common.loading")}</p>
      </div>
    );
  }

  if (entries.length === 0) {
    return (
      <div className="android-card android-panel">
        <p className="android-muted">
          {totalCount === 0
            ? t("settings.dictionary.empty")
            : t("settings.dictionary.noResults")}
        </p>
      </div>
    );
  }

  return (
    <div className="android-list android-library-list">
      {entries.map((entry) => {
        const isUpdating = updatingIds.has(entry.id);
        const isStarred = entry.priority === "starred";
        const source = entry.source ?? "manual";

        return (
          <article key={entry.id} className="android-library-card">
            <div className="android-card-header">
              <div className="android-library-main">
                <h2>{entry.phrase}</h2>
                <div className="android-chip-row">
                  <span className="android-badge">
                    {t(`settings.dictionary.source.${source}`)}
                  </span>
                  {entry.replacement_of && (
                    <span className="android-muted">
                      {t("settings.dictionary.corrects", {
                        replacement: entry.replacement_of,
                      })}
                    </span>
                  )}
                </div>
              </div>
              <button
                type="button"
                className="android-icon-button"
                disabled={isUpdating}
                onClick={() => onToggleStar(entry)}
                aria-label={t(
                  isStarred
                    ? "settings.dictionary.unstarEntry"
                    : "settings.dictionary.starEntry",
                  { phrase: entry.phrase },
                )}
              >
                <Star size={18} fill={isStarred ? "currentColor" : "none"} />
              </button>
            </div>
            <div className="android-actions">
              <button
                type="button"
                className="android-action"
                disabled={isUpdating}
                onClick={() => onEdit(entry)}
                aria-label={t("settings.dictionary.editEntry", {
                  phrase: entry.phrase,
                })}
              >
                <Pencil size={17} />
                <span>{t("common.edit")}</span>
              </button>
              <button
                type="button"
                className="android-action"
                disabled={isUpdating}
                onClick={() => onDelete(entry)}
                aria-label={t("settings.dictionary.deleteEntry", {
                  phrase: entry.phrase,
                })}
              >
                <Trash2 size={17} />
                <span>{t("common.delete")}</span>
              </button>
            </div>
          </article>
        );
      })}
    </div>
  );
}

function AndroidSnippetsPanel({ search }: { search: string }) {
  const { t } = useTranslation();
  const {
    entries,
    isLoading,
    updatingIds,
    loadEntries,
    addEntry,
    updateEntry,
    deleteEntry,
  } = useSnippetsStore();
  const [trigger, setTrigger] = useState("");
  const [content, setContent] = useState("");
  const [editingId, setEditingId] = useState<string | null>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    loadEntries().catch(() => {
      setError(t("settings.snippets.errors.load"));
    });
  }, [loadEntries, t]);

  const filteredEntries = useMemo(() => {
    const query = search.trim().toLowerCase();
    if (!query) return entries;

    return entries.filter((entry) =>
      [entry.trigger, entry.content].some((value) =>
        value.toLowerCase().includes(query),
      ),
    );
  }, [entries, search]);

  const resetForm = () => {
    setTrigger("");
    setContent("");
    setEditingId(null);
  };

  const handleSubmit = async (event: FormEvent) => {
    event.preventDefault();
    const nextTrigger = trigger.trim();
    const nextContent = content.trim();
    if (!nextTrigger || !nextContent || nextTrigger.length > 120) return;

    try {
      setError("");
      const input = { trigger: nextTrigger, content: nextContent };
      if (editingId) {
        await updateEntry(editingId, input);
      } else {
        await addEntry(input);
      }
      resetForm();
    } catch {
      setError(
        t(
          editingId
            ? "settings.snippets.errors.update"
            : "settings.snippets.errors.add",
        ),
      );
    }
  };

  const handleEdit = (entry: SnippetEntry) => {
    setTrigger(entry.trigger);
    setContent(entry.content);
    setEditingId(entry.id);
    setError("");
  };

  const handleDelete = async (entry: SnippetEntry) => {
    try {
      setError("");
      await deleteEntry(entry.id);
      if (editingId === entry.id) {
        resetForm();
      }
    } catch {
      setError(t("settings.snippets.errors.delete"));
    }
  };

  return (
    <section className="android-section">
      <div className="android-section-header">
        <h2>{t("settings.snippets.title")}</h2>
        <span className="android-badge">
          {t("settings.snippets.counts.total", { count: entries.length })}
        </span>
      </div>

      <form className="android-library-form" onSubmit={handleSubmit}>
        <label className="android-field">
          <span>{t("settings.snippets.trigger")}</span>
          <input
            value={trigger}
            onChange={(event) => setTrigger(event.target.value)}
            maxLength={120}
          />
        </label>
        <label className="android-field">
          <span>{t("settings.snippets.content")}</span>
          <textarea
            value={content}
            onChange={(event) => setContent(event.target.value)}
            rows={4}
            maxLength={4000}
          />
        </label>
        <div className="android-actions">
          <button
            type="submit"
            className="android-action android-primary-action"
            disabled={
              trigger.trim().length === 0 || content.trim().length === 0
            }
          >
            <Check size={17} />
            <span>
              {editingId
                ? t("settings.snippets.save")
                : t("settings.snippets.add")}
            </span>
          </button>
          {editingId && (
            <button
              type="button"
              className="android-action"
              onClick={resetForm}
            >
              <X size={17} />
              <span>{t("settings.snippets.cancel")}</span>
            </button>
          )}
        </div>
      </form>

      {error && (
        <p className="android-error-text" role="alert">
          {error}
        </p>
      )}

      <AndroidSnippetList
        entries={filteredEntries}
        totalCount={entries.length}
        isLoading={isLoading}
        updatingIds={updatingIds}
        onEdit={handleEdit}
        onDelete={handleDelete}
      />
    </section>
  );
}

function AndroidSnippetList({
  entries,
  totalCount,
  isLoading,
  updatingIds,
  onEdit,
  onDelete,
}: {
  entries: SnippetEntry[];
  totalCount: number;
  isLoading: boolean;
  updatingIds: Set<string>;
  onEdit: (entry: SnippetEntry) => void;
  onDelete: (entry: SnippetEntry) => void;
}) {
  const { t } = useTranslation();

  if (isLoading) {
    return (
      <div className="android-card android-panel">
        <p className="android-muted">{t("common.loading")}</p>
      </div>
    );
  }

  if (entries.length === 0) {
    return (
      <div className="android-card android-panel">
        <p className="android-muted">
          {totalCount === 0
            ? t("settings.snippets.empty")
            : t("settings.snippets.noResults")}
        </p>
      </div>
    );
  }

  return (
    <div className="android-list android-library-list">
      {entries.map((entry) => {
        const isUpdating = updatingIds.has(entry.id);
        const preview = entry.content.replace(/\s+/g, " ").trim();

        return (
          <article key={entry.id} className="android-library-card">
            <div className="android-card-header">
              <div className="android-library-main">
                <h2>{entry.trigger}</h2>
                <p className="android-muted">{preview}</p>
              </div>
              <Sparkles size={18} />
            </div>
            <div className="android-actions">
              <button
                type="button"
                className="android-action"
                disabled={isUpdating}
                onClick={() => onEdit(entry)}
                aria-label={t("settings.snippets.editEntry", {
                  trigger: entry.trigger,
                })}
              >
                <Pencil size={17} />
                <span>{t("common.edit")}</span>
              </button>
              <button
                type="button"
                className="android-action"
                disabled={isUpdating}
                onClick={() => onDelete(entry)}
                aria-label={t("settings.snippets.deleteEntry", {
                  trigger: entry.trigger,
                })}
              >
                <Trash2 size={17} />
                <span>{t("common.delete")}</span>
              </button>
            </div>
          </article>
        );
      })}
    </div>
  );
}

function ModelsTab() {
  const { t } = useTranslation();
  const {
    models,
    currentModel,
    downloadingModels,
    downloadProgress,
    selectModel,
    downloadModel,
    cancelDownload,
    deleteModel,
  } = useModelStore();

  const downloaded = models.filter(
    (model) => model.is_downloaded || model.id in downloadingModels,
  );
  const available = models.filter(
    (model) => !model.is_downloaded && !(model.id in downloadingModels),
  );
  const storageMb = downloaded.reduce(
    (total, model) => total + (model.is_downloaded ? model.size_mb : 0),
    0,
  );

  return (
    <>
      <div className="android-section-header">
        <button type="button" className="android-chip">
          <SlidersHorizontal size={16} />
          <span>{t("settings.models.filters.allLanguages")}</span>
        </button>
        <span className="android-badge">
          {t("android.models.storage", { count: Math.round(storageMb) })}
        </span>
      </div>

      <ModelSection
        title={t("android.models.downloaded")}
        models={downloaded}
        currentModel={currentModel}
        downloadingModels={downloadingModels}
        downloadProgress={downloadProgress}
        onSelect={selectModel}
        onDownload={downloadModel}
        onCancel={cancelDownload}
        onDelete={deleteModel}
      />
      <ModelSection
        title={t("android.models.available")}
        models={available}
        currentModel={currentModel}
        downloadingModels={downloadingModels}
        downloadProgress={downloadProgress}
        onSelect={selectModel}
        onDownload={downloadModel}
        onCancel={cancelDownload}
        onDelete={deleteModel}
      />
    </>
  );
}

function ModelSection({
  title,
  models,
  currentModel,
  downloadingModels,
  downloadProgress,
  onSelect,
  onDownload,
  onCancel,
  onDelete,
}: {
  title: string;
  models: ModelInfo[];
  currentModel: string;
  downloadingModels: Record<string, true>;
  downloadProgress: Record<string, { percentage: number }>;
  onSelect: (modelId: string) => Promise<boolean>;
  onDownload: (modelId: string) => Promise<boolean>;
  onCancel: (modelId: string) => Promise<boolean>;
  onDelete: (modelId: string) => Promise<boolean>;
}) {
  const { t } = useTranslation();

  if (models.length === 0) {
    return null;
  }

  return (
    <section className="android-section">
      <div className="android-section-header">
        <h2>{title}</h2>
      </div>
      <div className="android-list">
        {models.map((model) => {
          const active = model.id === currentModel;
          const downloading = model.id in downloadingModels;
          const progress = downloadProgress[model.id]?.percentage ?? 0;
          return (
            <article
              key={model.id}
              className={`android-model-card ${
                active ? "android-active-model" : ""
              }`}
            >
              <div className="android-model-row">
                <div>
                  <h3>{model.name}</h3>
                  <p className="android-muted">{model.description}</p>
                </div>
                {active && (
                  <span className="android-badge">
                    {t("modelSelector.active")}
                  </span>
                )}
              </div>
              {downloading && (
                <p className="android-muted">
                  {t("modelSelector.downloading", {
                    percentage: Math.round(progress),
                  })}
                </p>
              )}
              <div className="android-actions">
                {downloading ? (
                  <button
                    type="button"
                    className="android-action"
                    onClick={() => onCancel(model.id)}
                  >
                    {t("modelSelector.cancel")}
                  </button>
                ) : model.is_downloaded ? (
                  <>
                    {!active && (
                      <button
                        type="button"
                        className="android-action android-primary-action"
                        onClick={() => onSelect(model.id)}
                      >
                        {t("android.actions.switchModel")}
                      </button>
                    )}
                    {!active && (
                      <button
                        type="button"
                        className="android-action"
                        onClick={() => onDelete(model.id)}
                      >
                        {t("common.delete")}
                      </button>
                    )}
                  </>
                ) : (
                  <button
                    type="button"
                    className="android-action android-primary-action"
                    onClick={() => onDownload(model.id)}
                  >
                    {t("onboarding.download")}
                  </button>
                )}
              </div>
            </article>
          );
        })}
      </div>
    </section>
  );
}

function AndroidPostProcessingScreen() {
  const { t } = useTranslation();
  const {
    settings,
    updateSetting,
    setPostProcessProvider,
    updatePostProcessBaseUrl,
    updatePostProcessApiKey,
    updatePostProcessModel,
    fetchPostProcessModels,
    postProcessModelOptions,
    refreshSettings,
    isUpdating,
  } = useSettings();
  const [promptEditor, setPromptEditor] = useState<PromptEditorState | null>(
    null,
  );

  const providers = useMemo(
    () =>
      (settings?.post_process_providers ?? []).filter(
        isAndroidPostProcessProvider,
      ),
    [settings?.post_process_providers],
  );
  const selectedProvider = useMemo(
    () =>
      providers.find(
        (provider) => provider.id === settings?.post_process_provider_id,
      ) ?? providers[0],
    [providers, settings?.post_process_provider_id],
  );
  const selectedProviderId = selectedProvider?.id ?? "";
  const selectedProviderBaseUrl = selectedProvider?.base_url ?? "";
  const postProcessingEnabled = !!settings?.post_process_enabled;
  const prompts = settings?.post_process_prompts ?? [];
  const selectedPromptId = settings?.post_process_selected_prompt_id ?? "";
  const selectedPrompt =
    prompts.find((prompt) => prompt.id === selectedPromptId) ?? null;
  const configuredModel =
    settings?.post_process_models?.[selectedProviderId] ?? "";
  const configuredApiKey =
    settings?.post_process_api_keys?.[selectedProviderId] ?? "";
  const modelOptions = useMemo(() => {
    const seen = new Set<string>();
    const options: string[] = [];
    const addOption = (value: string | null | undefined) => {
      const trimmed = value?.trim();
      if (!trimmed || seen.has(trimmed)) return;
      seen.add(trimmed);
      options.push(trimmed);
    };
    for (const option of postProcessModelOptions[selectedProviderId] ?? []) {
      addOption(option);
    }
    addOption(configuredModel);
    return options;
  }, [configuredModel, postProcessModelOptions, selectedProviderId]);
  const [baseUrlDraft, setBaseUrlDraft] = useState(selectedProviderBaseUrl);
  const [apiKeyDraft, setApiKeyDraft] = useState(configuredApiKey);
  const [modelDraft, setModelDraft] = useState(configuredModel);
  const modelOptionsId = `android-post-process-models-${
    selectedProviderId || "none"
  }`;
  const modelInputLabel = t("settings.postProcessing.api.model.title");

  useEffect(() => {
    setBaseUrlDraft(selectedProviderBaseUrl);
  }, [selectedProviderBaseUrl, selectedProviderId]);

  useEffect(() => {
    setApiKeyDraft(configuredApiKey);
  }, [configuredApiKey, selectedProviderId]);

  useEffect(() => {
    setModelDraft(configuredModel);
  }, [configuredModel, selectedProviderId]);

  const handleProviderSelect = async (provider: PostProcessProvider) => {
    if (provider.id === selectedProviderId) return;
    await setPostProcessProvider(provider.id);
    if (
      (settings?.post_process_api_keys?.[provider.id] ?? "").trim() ||
      provider.base_url.trim()
    ) {
      void fetchPostProcessModels(provider.id);
    }
  };

  const handleBaseUrlBlur = async () => {
    const trimmed = baseUrlDraft.trim();
    if (
      !selectedProvider?.allow_base_url_edit ||
      trimmed === selectedProviderBaseUrl
    ) {
      return;
    }
    await updatePostProcessBaseUrl(selectedProvider.id, trimmed);
  };

  const handleApiKeyBlur = async () => {
    const trimmed = apiKeyDraft.trim();
    if (!selectedProviderId || trimmed === configuredApiKey) return;
    await updatePostProcessApiKey(selectedProviderId, trimmed);
  };

  const handleModelBlur = async () => {
    const trimmed = modelDraft.trim();
    if (!selectedProviderId || trimmed === configuredModel) return;
    await updatePostProcessModel(selectedProviderId, trimmed);
  };

  if (promptEditor) {
    return (
      <AndroidPostProcessPromptEditor
        editor={promptEditor}
        selectedPromptId={selectedPromptId}
        refreshSettings={refreshSettings}
        onClose={() => setPromptEditor(null)}
      />
    );
  }

  return (
    <>
      <section className="android-section">
        <div className="android-card android-panel android-post-process-summary">
          <div className="android-card-header">
            <div className="android-library-main">
              <h2>{t("settings.debug.postProcessingToggle.label")}</h2>
              <p className="android-muted">
                {t("settings.debug.postProcessingToggle.description")}
              </p>
            </div>
            <Switch
              checked={postProcessingEnabled}
              label={t("settings.debug.postProcessingToggle.label")}
              onClick={() =>
                updateSetting("post_process_enabled", !postProcessingEnabled)
              }
            />
          </div>
          <span
            className={`android-status-pill ${
              postProcessingEnabled
                ? "android-status-trust"
                : "android-status-warning"
            }`}
          >
            {t(postProcessingEnabled ? "common.enabled" : "common.disabled")}
          </span>
        </div>
      </section>

      <section className="android-section">
        <div className="android-section-header">
          <h2>{t("settings.postProcessing.api.provider.title")}</h2>
        </div>
        <div className="android-list">
          {providers.map((provider) => {
            const active = provider.id === selectedProviderId;
            return (
              <button
                key={provider.id}
                type="button"
                className={`android-list-row android-post-process-option ${
                  active ? "android-active-option" : ""
                }`}
                aria-pressed={active}
                disabled={isUpdating("post_process_provider_id")}
                onClick={() => void handleProviderSelect(provider)}
              >
                <div className="android-library-main">
                  <h3>{provider.label}</h3>
                  <p className="android-muted">{provider.base_url}</p>
                </div>
                {active && <Check size={18} />}
              </button>
            );
          })}
          {providers.length === 0 && (
            <div className="android-list-row">
              <p className="android-muted">{t("common.noOptionsFound")}</p>
            </div>
          )}
        </div>
      </section>

      {selectedProvider && (
        <section className="android-section">
          <div className="android-section-header">
            <h2>{t("settings.postProcessing.api.title")}</h2>
          </div>
          <div className="android-library-form">
            <label className="android-field">
              <span>{t("settings.postProcessing.api.baseUrl.title")}</span>
              <input
                type="url"
                value={baseUrlDraft}
                aria-label={t("settings.postProcessing.api.baseUrl.title")}
                disabled={!selectedProvider.allow_base_url_edit}
                onChange={(event) => setBaseUrlDraft(event.target.value)}
                onBlur={() => void handleBaseUrlBlur()}
                placeholder={t(
                  "settings.postProcessing.api.baseUrl.placeholder",
                )}
              />
            </label>
            <label className="android-field">
              <span>{t("settings.postProcessing.api.apiKey.title")}</span>
              <input
                type="password"
                value={apiKeyDraft}
                aria-label={t("settings.postProcessing.api.apiKey.title")}
                autoComplete="off"
                onChange={(event) => setApiKeyDraft(event.target.value)}
                onBlur={() => void handleApiKeyBlur()}
                placeholder={t(
                  "settings.postProcessing.api.apiKey.placeholder",
                )}
              />
            </label>
            <div className="android-field">
              <span>{modelInputLabel}</span>
              <div className="android-field-row">
                <input
                  value={modelDraft}
                  aria-label={modelInputLabel}
                  list={modelOptionsId}
                  onChange={(event) => setModelDraft(event.target.value)}
                  onBlur={() => void handleModelBlur()}
                  placeholder={t(
                    modelOptions.length > 0
                      ? "settings.postProcessing.api.model.placeholderWithOptions"
                      : "settings.postProcessing.api.model.placeholderNoOptions",
                  )}
                />
                <button
                  type="button"
                  className="android-icon-button"
                  aria-label={t(
                    "settings.postProcessing.api.model.refreshModels",
                  )}
                  disabled={
                    !selectedProviderId ||
                    isUpdating(
                      `post_process_models_fetch:${selectedProviderId}`,
                    )
                  }
                  onClick={() =>
                    void fetchPostProcessModels(selectedProviderId)
                  }
                >
                  <RefreshCw size={18} />
                </button>
              </div>
              <datalist id={modelOptionsId}>
                {modelOptions.map((option) => (
                  <option key={option} value={option} />
                ))}
              </datalist>
            </div>
          </div>
        </section>
      )}

      <section className="android-section">
        <div className="android-section-header">
          <h2>{t("settings.postProcessing.prompts.title")}</h2>
        </div>
        <div className="android-library-form">
          <label className="android-field">
            <span>
              {t("settings.postProcessing.prompts.selectedPrompt.title")}
            </span>
            <select
              value={selectedPromptId}
              aria-label={t(
                "settings.postProcessing.prompts.selectedPrompt.title",
              )}
              onChange={(event) =>
                updateSetting(
                  "post_process_selected_prompt_id",
                  event.target.value,
                )
              }
            >
              <option value="">
                {prompts.length === 0
                  ? t("settings.postProcessing.prompts.noPrompts")
                  : t("settings.postProcessing.prompts.selectPrompt")}
              </option>
              {prompts.map((prompt) => (
                <option key={prompt.id} value={prompt.id}>
                  {prompt.name}
                </option>
              ))}
            </select>
          </label>
          <div className="android-actions">
            <button
              type="button"
              className="android-action android-primary-action"
              onClick={() => setPromptEditor({ mode: "create" })}
            >
              <Sparkles size={17} />
              <span>{t("settings.postProcessing.prompts.createNew")}</span>
            </button>
            {selectedPrompt && (
              <button
                type="button"
                className="android-action"
                onClick={() =>
                  setPromptEditor({ mode: "edit", prompt: selectedPrompt })
                }
              >
                <Pencil size={17} />
                <span>{t("common.edit")}</span>
              </button>
            )}
          </div>
        </div>
      </section>
    </>
  );
}

function AndroidPostProcessPromptEditor({
  editor,
  selectedPromptId,
  refreshSettings,
  onClose,
}: {
  editor: PromptEditorState;
  selectedPromptId: string;
  refreshSettings: () => Promise<void>;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [name, setName] = useState(
    editor.mode === "edit" ? editor.prompt.name : "",
  );
  const [prompt, setPrompt] = useState(
    editor.mode === "edit" ? editor.prompt.prompt : "",
  );
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState("");

  const handleSave = async (event: FormEvent) => {
    event.preventDefault();
    const nextName = name.trim();
    const nextPrompt = prompt.trim();
    if (!nextName || !nextPrompt) return;

    setIsSaving(true);
    setError("");
    try {
      if (editor.mode === "edit") {
        const result = await commands.updatePostProcessPrompt(
          editor.prompt.id,
          nextName,
          nextPrompt,
        );
        if (result.status === "error") {
          setError(result.error);
          return;
        }
      } else {
        const result = await commands.addPostProcessPrompt(
          nextName,
          nextPrompt,
        );
        if (result.status === "error") {
          setError(result.error);
          return;
        }
        await commands.setPostProcessSelectedPrompt(result.data.id);
      }
      await refreshSettings();
      onClose();
    } catch (saveError) {
      setError(
        saveError instanceof Error ? saveError.message : String(saveError),
      );
    } finally {
      setIsSaving(false);
    }
  };

  const handleDelete = async () => {
    if (editor.mode !== "edit") return;

    setIsSaving(true);
    setError("");
    try {
      const result = await commands.deletePostProcessPrompt(editor.prompt.id);
      if (result.status === "error") {
        setError(result.error);
        return;
      }
      if (selectedPromptId === editor.prompt.id) {
        await commands.setPostProcessSelectedPrompt("");
      }
      await refreshSettings();
      onClose();
    } catch (deleteError) {
      setError(
        deleteError instanceof Error
          ? deleteError.message
          : String(deleteError),
      );
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <section className="android-section android-post-process-editor">
      <div className="android-section-header">
        <h2>
          {editor.mode === "edit"
            ? t("common.edit")
            : t("settings.postProcessing.prompts.createNew")}
        </h2>
      </div>
      <form className="android-library-form" onSubmit={handleSave}>
        <label className="android-field">
          <span>{t("settings.postProcessing.prompts.promptLabel")}</span>
          <input
            value={name}
            maxLength={80}
            onChange={(event) => setName(event.target.value)}
            placeholder={t(
              "settings.postProcessing.prompts.promptLabelPlaceholder",
            )}
          />
        </label>
        <label className="android-field">
          <span>{t("settings.postProcessing.prompts.promptInstructions")}</span>
          <textarea
            value={prompt}
            onChange={(event) => setPrompt(event.target.value)}
            placeholder={t(
              "settings.postProcessing.prompts.promptInstructionsPlaceholder",
            )}
          />
        </label>
        {error && (
          <p className="android-error-text" role="alert">
            {error}
          </p>
        )}
        <div className="android-actions">
          <button
            type="submit"
            className="android-action android-primary-action"
            disabled={isSaving || !name.trim() || !prompt.trim()}
          >
            <Check size={17} />
            <span>{t("common.save")}</span>
          </button>
          <button
            type="button"
            className="android-action"
            disabled={isSaving}
            onClick={onClose}
          >
            <X size={17} />
            <span>{t("common.cancel")}</span>
          </button>
          {editor.mode === "edit" && (
            <button
              type="button"
              className="android-action"
              disabled={isSaving}
              onClick={() => void handleDelete()}
            >
              <Trash2 size={17} />
              <span>{t("common.delete")}</span>
            </button>
          )}
        </div>
      </form>
    </section>
  );
}

function SettingsTab({
  theme,
  setTheme,
  openLibrary,
  openPostProcessing,
  openAbout,
}: {
  theme: AndroidTheme;
  setTheme: (theme: AndroidTheme) => void;
  openLibrary: (section: LibrarySection) => void;
  openPostProcessing: () => void;
  openAbout: () => void;
}) {
  const { t, i18n } = useTranslation();
  const {
    settings,
    updateSetting,
    audioDevices,
    outputDevices,
    audioFeedbackEnabled,
    isUpdating,
    refreshAudioDevices,
    refreshOutputDevices,
  } = useSettings();
  const customSounds = useSettingsStore((state) => state.customSounds);
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [settingsSheet, setSettingsSheet] = useState<SettingsSheet | null>(
    null,
  );
  const [historyLimitDraft, setHistoryLimitDraft] = useState("100");
  const [bubbleCorner, setBubbleCorner] =
    useState<AndroidBubbleCorner>("top-right");
  const [version, setVersion] = useState("");

  const selectedMicrophone = normalizeDeviceSetting(
    settings?.selected_microphone,
  );
  const selectedOutput = normalizeDeviceSetting(
    settings?.selected_output_device,
  );
  const microphoneOptions = useMemo(
    () => deviceOptions(audioDevices, t("common.default")),
    [audioDevices, t],
  );
  const outputOptions = useMemo(
    () => deviceOptions(outputDevices, t("common.default")),
    [outputDevices, t],
  );
  const rawVolume = settings?.audio_feedback_volume ?? 0.5;
  const volumePercent = Math.round(
    rawVolume <= 1 ? rawVolume * 100 : rawVolume,
  );
  const currentLanguage = (settings?.app_language ||
    i18n.language) as SupportedLanguageCode;
  const currentLanguageMeta =
    SUPPORTED_LANGUAGES.find(
      (language) =>
        language.code.toLowerCase() === currentLanguage.toLowerCase(),
    ) ??
    SUPPORTED_LANGUAGES.find(
      (language) =>
        language.code.toLowerCase() ===
        currentLanguage.toLowerCase().split("-")[0],
    );
  const currentLanguageCode = currentLanguageMeta?.code || currentLanguage;
  const currentLanguageLabel =
    currentLanguageMeta?.nativeName || currentLanguage;
  const historyLimit = Number(settings?.history_limit ?? 100);
  const selectedRetention = normalizeRetentionPeriod(
    settings?.recording_retention_period,
  );
  const selectedSoundTheme = settings?.sound_theme || "marimba";
  const selectedMicrophoneLabel =
    selectedMicrophone === DEFAULT_DEVICE_VALUE
      ? t("common.default")
      : audioDevices.find((device) => device.name === selectedMicrophone)
          ?.name || selectedMicrophone;
  const selectedOutputLabel =
    selectedOutput === DEFAULT_DEVICE_VALUE
      ? t("common.default")
      : outputDevices.find((device) => device.name === selectedOutput)?.name ||
        selectedOutput;
  const visibleSoundThemeOptions = useMemo(
    () =>
      soundThemeOptions.filter(
        (option) =>
          option !== "custom" ||
          selectedSoundTheme === "custom" ||
          (customSounds.start && customSounds.stop),
      ),
    [customSounds.start, customSounds.stop, selectedSoundTheme],
  );
  const bubbleCornerLabels: Record<AndroidBubbleCorner, string> = {
    "top-left": t("android.settings.bubblePosition.topLeft"),
    "top-right": t("android.settings.bubblePosition.topRight"),
    "bottom-left": t("android.settings.bubblePosition.bottomLeft"),
    "bottom-right": t("android.settings.bubblePosition.bottomRight"),
  };

  useEffect(() => {
    getDisplayVersion()
      .then(setVersion)
      .catch(() => setVersion("0.8.8"));
  }, []);

  useEffect(() => {
    setBubbleCorner(
      normalizeBubbleCorner(
        safeBridge()?.bubbleCornerSnapshot?.() ?? settings?.overlay_position,
      ),
    );
  }, [settings?.overlay_position]);

  useEffect(() => {
    if (settingsSheet === "microphone") {
      void refreshAudioDevices();
    }
    if (settingsSheet === "output") {
      void refreshOutputDevices();
    }
    if (settingsSheet === "historyLimit") {
      setHistoryLimitDraft(String(historyLimit));
    }
  }, [historyLimit, refreshAudioDevices, refreshOutputDevices, settingsSheet]);

  const closeSheet = () => setSettingsSheet(null);

  const handleDeviceSelect = async (
    setting: "selected_microphone" | "selected_output_device",
    value: string,
  ) => {
    await updateSetting(setting, value);
    closeSheet();
  };

  const handleVolumeChange = (value: number) => {
    void updateSetting("audio_feedback_volume", value / 100);
  };

  const handleBubbleCornerSelect = (corner: AndroidBubbleCorner) => {
    setBubbleCorner(corner);
    safeBridge()?.setBubbleCorner?.(corner);
    closeSheet();
  };

  const handleLanguageSelect = async (language: SupportedLanguageCode) => {
    await changeAppLanguage(language);
    await updateSetting("app_language", language);
    closeSheet();
  };

  const handleHistoryLimitSave = async () => {
    const parsed = Number(historyLimitDraft);
    const limit = Number.isFinite(parsed)
      ? Math.max(1, Math.min(10000, Math.round(parsed)))
      : historyLimit;
    await updateSetting("history_limit", limit);
    closeSheet();
  };

  const handleRetentionSelect = async (period: AndroidRetentionPeriod) => {
    await updateSetting(
      "recording_retention_period",
      period as RecordingRetentionPeriod,
    );
    closeSheet();
  };

  const handleSoundThemeSelect = async (soundTheme: SoundTheme) => {
    await updateSetting("sound_theme", soundTheme);
    closeSheet();
  };

  const retentionLabel = (period: AndroidRetentionPeriod) => {
    switch (period) {
      case "preserve_limit":
        return t("settings.debug.recordingRetention.preserveLimit", {
          count: historyLimit,
        });
      case "days3":
        return t("settings.debug.recordingRetention.days3");
      case "weeks2":
        return t("settings.debug.recordingRetention.weeks2");
      case "months3":
        return t("settings.debug.recordingRetention.months3");
      case "never":
      default:
        return t("settings.debug.recordingRetention.never");
    }
  };

  const soundThemeLabel = (soundTheme: SoundTheme) =>
    t(`android.settings.soundTheme.${soundTheme}`);

  const renderPickerOption = ({
    selected,
    label,
    description,
    optionKey,
    onClick,
  }: {
    selected: boolean;
    label: string;
    description?: string;
    optionKey?: string;
    onClick: () => void;
  }) => (
    <button
      type="button"
      key={optionKey ?? label}
      className={`android-picker-option ${
        selected ? "android-picker-option-active" : ""
      }`}
      onClick={onClick}
    >
      <span className="android-picker-copy">
        <span>{label}</span>
        {description && <span className="android-muted">{description}</span>}
      </span>
      {selected ? <Check size={18} /> : <ChevronRight size={18} />}
    </button>
  );

  const renderSettingsSheet = () => {
    if (!settingsSheet) {
      return null;
    }

    if (settingsSheet === "microphone") {
      return (
        <AndroidSettingsSheet
          title={t("settings.sound.microphone.title")}
          onClose={closeSheet}
        >
          <button
            type="button"
            className="android-action android-sheet-action"
            onClick={() => void refreshAudioDevices()}
          >
            <RefreshCw size={17} />
            <span>{t("android.settings.refreshDevices")}</span>
          </button>
          <div className="android-picker-list">
            {microphoneOptions.map((option) =>
              renderPickerOption({
                selected: selectedMicrophone === option.value,
                label: option.label,
                description: option.description,
                optionKey: option.value,
                onClick: () =>
                  void handleDeviceSelect("selected_microphone", option.value),
              }),
            )}
          </div>
        </AndroidSettingsSheet>
      );
    }

    if (settingsSheet === "output") {
      return (
        <AndroidSettingsSheet
          title={t("settings.sound.outputDevice.title")}
          onClose={closeSheet}
        >
          <button
            type="button"
            className="android-action android-sheet-action"
            onClick={() => void refreshOutputDevices()}
          >
            <RefreshCw size={17} />
            <span>{t("android.settings.refreshDevices")}</span>
          </button>
          <div className="android-picker-list">
            {outputOptions.map((option) =>
              renderPickerOption({
                selected: selectedOutput === option.value,
                label: option.label,
                description: option.description,
                optionKey: option.value,
                onClick: () =>
                  void handleDeviceSelect(
                    "selected_output_device",
                    option.value,
                  ),
              }),
            )}
          </div>
        </AndroidSettingsSheet>
      );
    }

    if (settingsSheet === "bubblePosition") {
      return (
        <AndroidSettingsSheet
          title={t("android.settings.bubblePosition.title")}
          onClose={closeSheet}
        >
          <div className="android-corner-grid">
            {androidBubbleCorners.map((corner) => (
              <button
                type="button"
                key={corner}
                className={`android-corner-option ${
                  bubbleCorner === corner ? "android-picker-option-active" : ""
                }`}
                onClick={() => handleBubbleCornerSelect(corner)}
              >
                <span className="android-corner-preview">
                  <span
                    className={`android-corner-dot android-corner-${corner}`}
                  />
                </span>
                <span>{bubbleCornerLabels[corner]}</span>
                {bubbleCorner === corner && <Check size={18} />}
              </button>
            ))}
          </div>
        </AndroidSettingsSheet>
      );
    }

    if (settingsSheet === "appLanguage") {
      return (
        <AndroidSettingsSheet
          title={t("appLanguage.title")}
          onClose={closeSheet}
        >
          <div className="android-picker-list">
            {SUPPORTED_LANGUAGES.map((language) =>
              renderPickerOption({
                selected: language.code === currentLanguageCode,
                label: `${language.nativeName} (${language.name})`,
                optionKey: language.code,
                onClick: () =>
                  void handleLanguageSelect(
                    language.code as SupportedLanguageCode,
                  ),
              }),
            )}
          </div>
        </AndroidSettingsSheet>
      );
    }

    if (settingsSheet === "historyLimit") {
      return (
        <AndroidSettingsSheet
          title={t("settings.debug.historyLimit.title")}
          onClose={closeSheet}
        >
          <form
            className="android-library-form"
            onSubmit={(event) => {
              event.preventDefault();
              void handleHistoryLimitSave();
            }}
          >
            <label className="android-field">
              <span>{t("settings.debug.historyLimit.title")}</span>
              <input
                type="number"
                min="1"
                max="10000"
                inputMode="numeric"
                value={historyLimitDraft}
                onChange={(event) => setHistoryLimitDraft(event.target.value)}
              />
            </label>
            <button
              type="submit"
              className="android-primary-action"
              disabled={isUpdating("history_limit")}
            >
              {t("common.save")}
            </button>
          </form>
        </AndroidSettingsSheet>
      );
    }

    if (settingsSheet === "recordingRetention") {
      return (
        <AndroidSettingsSheet
          title={t("settings.debug.recordingRetention.title")}
          onClose={closeSheet}
        >
          <div className="android-picker-list">
            {retentionPeriods.map((period) =>
              renderPickerOption({
                selected: selectedRetention === period,
                label: retentionLabel(period),
                optionKey: period,
                onClick: () => void handleRetentionSelect(period),
              }),
            )}
          </div>
        </AndroidSettingsSheet>
      );
    }

    return (
      <AndroidSettingsSheet
        title={t("settings.debug.soundTheme.label")}
        onClose={closeSheet}
      >
        <div className="android-picker-list">
          {visibleSoundThemeOptions.map((option) =>
            renderPickerOption({
              selected: selectedSoundTheme === option,
              label: soundThemeLabel(option),
              optionKey: option,
              onClick: () => void handleSoundThemeSelect(option),
            }),
          )}
        </div>
      </AndroidSettingsSheet>
    );
  };

  return (
    <>
      <section className="android-section">
        <div className="android-section-header">
          <h2>{t("android.settings.general")}</h2>
        </div>
        <div className="android-settings-group">
          <button
            type="button"
            className="android-settings-row"
            onClick={() => setSettingsSheet("microphone")}
          >
            <span>{t("settings.sound.microphone.title")}</span>
            <span className="android-muted">{selectedMicrophoneLabel}</span>
            <ChevronRight size={18} />
          </button>
          <div className="android-settings-row">
            <span>{t("settings.sound.audioFeedback.label")}</span>
            <Switch
              checked={!!settings?.audio_feedback}
              label={t("settings.sound.audioFeedback.label")}
              onClick={() =>
                updateSetting("audio_feedback", !settings?.audio_feedback)
              }
            />
          </div>
          <div className="android-settings-row android-settings-row-stacked">
            <div className="android-settings-row-header">
              <span>{t("settings.sound.volume.title")}</span>
              <span className="android-muted">
                {audioFeedbackEnabled
                  ? t("android.settings.percent", { value: volumePercent })
                  : t("common.disabled")}
              </span>
            </div>
            <input
              type="range"
              min="0"
              max="100"
              value={volumePercent}
              aria-label={t("settings.sound.volume.title")}
              className="android-range"
              disabled={!audioFeedbackEnabled}
              onChange={(event) =>
                handleVolumeChange(Number(event.target.value))
              }
            />
          </div>
          <div className="android-settings-row">
            <span>{t("settings.debug.muteWhileRecording.label")}</span>
            <Switch
              checked={!!settings?.mute_while_recording}
              label={t("settings.debug.muteWhileRecording.label")}
              onClick={() =>
                updateSetting(
                  "mute_while_recording",
                  !settings?.mute_while_recording,
                )
              }
            />
          </div>
          <button
            type="button"
            className="android-settings-row"
            onClick={() => setSettingsSheet("bubblePosition")}
          >
            <span>{t("android.settings.bubblePosition.title")}</span>
            <span className="android-muted">
              {bubbleCornerLabels[bubbleCorner]}
            </span>
            <MapPin size={18} />
          </button>
          <div className="android-settings-row">
            <span>{t("settings.debug.appendTrailingSpace.label")}</span>
            <Switch
              checked={!!settings?.append_trailing_space}
              label={t("settings.debug.appendTrailingSpace.label")}
              onClick={() =>
                updateSetting(
                  "append_trailing_space",
                  !settings?.append_trailing_space,
                )
              }
            />
          </div>
        </div>
      </section>

      <section className="android-section">
        <div className="android-section-header">
          <h2>{t("android.settings.appearance")}</h2>
        </div>
        <div className="android-segments">
          {(["system", "light", "dark"] as AndroidTheme[]).map((option) => (
            <button
              type="button"
              key={option}
              className={`android-segment ${
                theme === option ? "android-segment-active" : ""
              }`}
              onClick={() => setTheme(option)}
            >
              {t(`android.settings.theme.${option}`)}
            </button>
          ))}
        </div>
        <div className="android-settings-group android-settings-group-spaced">
          <button
            type="button"
            className="android-settings-row"
            onClick={() => setSettingsSheet("appLanguage")}
          >
            <span>{t("appLanguage.title")}</span>
            <span className="android-muted">{currentLanguageLabel}</span>
            <Languages size={18} />
          </button>
        </div>
      </section>

      <section className="android-section">
        <button
          type="button"
          className="android-settings-row android-panel"
          onClick={() => setAdvancedOpen((open) => !open)}
        >
          <span>{t("android.settings.advanced")}</span>
          <SlidersHorizontal size={20} />
        </button>
        {advancedOpen && (
          <div className="android-settings-group" style={{ marginTop: 10 }}>
            <button
              type="button"
              className="android-settings-row"
              onClick={() => openLibrary("dictionary")}
            >
              <span>{t("sidebar.dictionary")}</span>
              <BookOpen size={18} />
            </button>
            <button
              type="button"
              className="android-settings-row"
              onClick={() => openLibrary("snippets")}
            >
              <span>{t("sidebar.snippets")}</span>
              <Sparkles size={18} />
            </button>
            <button
              type="button"
              className="android-settings-row"
              onClick={openPostProcessing}
            >
              <span>{t("sidebar.postProcessing")}</span>
              <Sparkles size={18} />
            </button>
            <button
              type="button"
              className="android-settings-row"
              onClick={() => setSettingsSheet("historyLimit")}
            >
              <span>{t("settings.debug.historyLimit.title")}</span>
              <span className="android-muted">
                {historyLimit} {t("settings.debug.historyLimit.entries")}
              </span>
              <ChevronRight size={18} />
            </button>
            <button
              type="button"
              className="android-settings-row"
              onClick={() => setSettingsSheet("recordingRetention")}
            >
              <span>{t("settings.debug.recordingRetention.title")}</span>
              <span className="android-muted">
                {retentionLabel(selectedRetention)}
              </span>
              <ChevronRight size={18} />
            </button>
            <button
              type="button"
              className="android-settings-row"
              onClick={() => setSettingsSheet("soundTheme")}
            >
              <span>{t("settings.debug.soundTheme.label")}</span>
              <span className="android-muted">
                {soundThemeLabel(selectedSoundTheme)}
              </span>
              <Music size={18} />
            </button>
            <button
              type="button"
              className="android-settings-row"
              onClick={() => setSettingsSheet("output")}
            >
              <span>{t("settings.sound.outputDevice.title")}</span>
              <span className="android-muted">{selectedOutputLabel}</span>
              <Volume1 size={18} />
            </button>
          </div>
        )}
      </section>

      <section className="android-section">
        <div className="android-settings-group">
          <div className="android-settings-row">
            <span>{t("settings.about.version.title")}</span>
            <span className="android-muted">
              {version || t("common.loading")}
            </span>
          </div>
          <button
            type="button"
            className="android-settings-row"
            onClick={openAbout}
          >
            <span>{t("settings.about.title")}</span>
            <span className="android-muted">
              {t("settings.about.acknowledgments.title")}
            </span>
            <Info size={18} />
          </button>
          <button
            type="button"
            className="android-settings-row"
            onClick={() => openAndroidExternalUrl(VERBATIM_SOURCE_URL)}
          >
            <span>{t("settings.about.sourceCode.title")}</span>
            <span className="android-muted">
              {t("settings.about.sourceCode.button")}
            </span>
            <ExternalLink size={18} />
          </button>
        </div>
      </section>
      {renderSettingsSheet()}
    </>
  );
}

function AndroidAboutScreen() {
  const { t } = useTranslation();
  const [version, setVersion] = useState("");

  useEffect(() => {
    getDisplayVersion()
      .then(setVersion)
      .catch(() => setVersion("0.8.8"));
  }, []);

  return (
    <>
      <section className="android-section">
        <div className="android-section-header">
          <h2>{t("settings.about.title")}</h2>
        </div>
        <div className="android-settings-group">
          <div className="android-settings-row">
            <span>{t("settings.about.version.title")}</span>
            <span className="android-muted">
              {version || t("common.loading")}
            </span>
          </div>
          <button
            type="button"
            className="android-settings-row"
            onClick={() => openAndroidExternalUrl(VERBATIM_SOURCE_URL)}
          >
            <span>{t("settings.about.sourceCode.title")}</span>
            <span className="android-muted">
              {t("settings.about.sourceCode.button")}
            </span>
            <ExternalLink size={18} />
          </button>
        </div>
      </section>

      <section className="android-section">
        <div className="android-section-header">
          <h2>{t("settings.about.acknowledgments.title")}</h2>
        </div>
        <div className="android-card android-about-card">
          <div className="android-card-header">
            <div>
              <h2>{t("settings.about.acknowledgments.handy.title")}</h2>
              <p className="android-muted">
                {t("settings.about.acknowledgments.handy.description")}
              </p>
            </div>
            <Info size={20} />
          </div>
          <p>{t("settings.about.acknowledgments.handy.details")}</p>
          <button
            type="button"
            className="android-action"
            onClick={() => openAndroidExternalUrl(HANDY_SOURCE_URL)}
          >
            <ExternalLink size={17} />
            <span>{t("settings.about.acknowledgments.handy.button")}</span>
          </button>
        </div>

        <div className="android-card android-about-card">
          <div className="android-card-header">
            <div>
              <h2>{t("settings.about.acknowledgments.license.title")}</h2>
              <p className="android-muted">
                {t("settings.about.acknowledgments.license.description")}
              </p>
            </div>
            <FileText size={20} />
          </div>
          <p>{t("settings.about.acknowledgments.license.details")}</p>
        </div>

        <div className="android-card android-about-card">
          <div className="android-card-header">
            <div>
              <h2>{t("settings.about.acknowledgments.whisper.title")}</h2>
              <p className="android-muted">
                {t("settings.about.acknowledgments.whisper.description")}
              </p>
            </div>
            <Cpu size={20} />
          </div>
          <p>{t("settings.about.acknowledgments.whisper.details")}</p>
        </div>
      </section>
    </>
  );
}

function AndroidSettingsSheet({
  title,
  onClose,
  children,
}: {
  title: string;
  onClose: () => void;
  children: ReactNode;
}) {
  const { t } = useTranslation();

  return (
    <div className="android-sheet-backdrop">
      <section
        className="android-settings-sheet"
        role="dialog"
        aria-modal="true"
        aria-label={title}
      >
        <div className="android-sheet-header">
          <h2>{title}</h2>
          <button
            type="button"
            className="android-icon-button"
            aria-label={t("common.cancel")}
            onClick={onClose}
          >
            <X size={20} />
          </button>
        </div>
        {children}
      </section>
    </div>
  );
}
