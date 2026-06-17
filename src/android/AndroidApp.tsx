import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  BookOpen,
  Check,
  Copy,
  Cpu,
  History,
  Home,
  Mic,
  MicOff,
  Moon,
  Search,
  Settings,
  Share2,
  SlidersHorizontal,
  Sparkles,
  Star,
  Sun,
  Trash2,
  Volume2,
} from "lucide-react";
import { commands, type HistoryEntry, type ModelInfo } from "@/bindings";
import { useSettings } from "@/hooks/useSettings";
import { getDisplayVersion } from "@/lib/appVersion";
import { useModelStore } from "@/stores/modelStore";
import { formatDateTime } from "@/utils/dateFormat";
import "./AndroidApp.css";

type AndroidTab = "home" | "history" | "models" | "settings";
type AndroidTheme = "system" | "light" | "dark";

type AndroidPermissionSnapshot = {
  microphone: boolean;
  overlay: boolean;
  accessibility: boolean;
  bubbleRunning: boolean;
  speechRecognizerAvailable: boolean;
  onDeviceSpeechRecognizerAvailable: boolean;
};

declare global {
  interface Window {
    VerbatimAndroid?: {
      permissionSnapshot: () => string;
      requestMicrophone: () => void;
      openOverlaySettings: () => void;
      openAccessibilitySettings: () => void;
      startBubble: () => void;
      stopBubble: () => void;
    };
  }
}

const defaultPermissions: AndroidPermissionSnapshot = {
  microphone: false,
  overlay: false,
  accessibility: false,
  bubbleRunning: false,
  speechRecognizerAvailable: false,
  onDeviceSpeechRecognizerAvailable: false,
};

const tabs: Array<{ id: AndroidTab; labelKey: string; icon: typeof Home }> = [
  { id: "home", labelKey: "android.tabs.home", icon: Home },
  { id: "history", labelKey: "android.tabs.history", icon: History },
  { id: "models", labelKey: "android.tabs.models", icon: Cpu },
  { id: "settings", labelKey: "android.tabs.settings", icon: Settings },
];

const safeBridge = () => window.VerbatimAndroid;

const parsePermissions = (value: string): AndroidPermissionSnapshot => {
  try {
    return { ...defaultPermissions, ...JSON.parse(value) };
  } catch {
    return defaultPermissions;
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

export default function AndroidApp() {
  const { t, i18n } = useTranslation();
  const [activeTab, setActiveTab] = useState<AndroidTab>("home");
  const [theme, setTheme] = useState<AndroidTheme>(() => {
    const stored = window.localStorage.getItem("verbatim.android.theme");
    return stored === "light" || stored === "dark" || stored === "system"
      ? stored
      : "system";
  });
  const [permissions, setPermissions] =
    useState<AndroidPermissionSnapshot>(defaultPermissions);

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
    permissions.microphone && permissions.overlay && permissions.accessibility;

  const title = t(`android.tabs.${activeTab}`);

  return (
    <div className={`android-app android-theme-${theme}`}>
      <main className="android-shell">
        <header className="android-top-bar">
          <h1>{activeTab === "home" ? t("common.appName") : title}</h1>
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

        {!allPermissionsReady ? (
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
        ) : activeTab === "models" ? (
          <ModelsTab />
        ) : (
          <SettingsTab theme={theme} setTheme={setTheme} />
        )}
      </main>

      <nav className="android-nav" aria-label={t("android.tabs.navigation")}>
        {tabs.map((tab) => {
          const Icon = tab.icon;
          const active = activeTab === tab.id;
          return (
            <button
              type="button"
              key={tab.id}
              className={active ? "android-nav-active" : ""}
              onClick={() => setActiveTab(tab.id)}
            >
              <span className="android-nav-icon">
                <Icon size={22} />
              </span>
              <span>{t(tab.labelKey)}</span>
            </button>
          );
        })}
      </nav>
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
  ];

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
        {steps.map((step) => (
          <div className="android-permission-row" key={step.title}>
            <div className="android-permission-copy">
              <h3>{step.title}</h3>
              <p className="android-muted">{step.description}</p>
              {step.callout && (
                <div
                  className={`android-callout ${
                    step.ready
                      ? "android-callout-trust"
                      : "android-callout-warning"
                  }`}
                >
                  {step.callout}
                </div>
              )}
            </div>
            {step.ready ? (
              <Check aria-label={t("android.permissions.granted")} size={24} />
            ) : (
              <button
                type="button"
                className="android-action android-primary-action"
                onClick={() => {
                  step.onClick();
                  window.setTimeout(refreshPermissions, 600);
                }}
              >
                {step.action}
              </button>
            )}
          </div>
        ))}
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

  useEffect(() => {
    commands.getHistoryEntries(null, 1).then((result) => {
      if (result.status === "ok") {
        setLastEntry(result.data.entries[0] ?? null);
      }
    });
  }, []);

  const bubbleReady =
    permissions.microphone && permissions.overlay && permissions.accessibility;

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
          <span>{t("android.home.bubble.toggle")}</span>
          <Switch
            checked={permissions.bubbleRunning}
            label={t("android.home.bubble.toggle")}
            onClick={() =>
              permissions.bubbleRunning
                ? safeBridge()?.stopBubble()
                : safeBridge()?.startBubble()
            }
          />
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
          {lastEntry?.transcription_text ||
            t("android.home.lastTranscript.empty")}
        </p>
        {lastEntry && (
          <div className="android-actions">
            <button
              type="button"
              className="android-action"
              onClick={() =>
                navigator.clipboard.writeText(lastEntry.transcription_text)
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

  useEffect(() => {
    commands.getHistoryEntries(null, 30).then((result) => {
      if (result.status === "ok") {
        setEntries(result.data.entries);
      }
    });
  }, []);

  const filteredEntries = entries.filter((entry) =>
    entry.transcription_text.toLowerCase().includes(search.toLowerCase()),
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
                <p className="android-transcript">{entry.transcription_text}</p>
                {expanded && (
                  <div className="android-actions">
                    <button
                      type="button"
                      className="android-action"
                      onClick={(event) => {
                        event.stopPropagation();
                        navigator.clipboard.writeText(entry.transcription_text);
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

function SettingsTab({
  theme,
  setTheme,
}: {
  theme: AndroidTheme;
  setTheme: (theme: AndroidTheme) => void;
}) {
  const { t } = useTranslation();
  const {
    settings,
    updateSetting,
    audioDevices,
    outputDevices,
    audioFeedbackEnabled,
  } = useSettings();
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [version, setVersion] = useState("");

  const selectedMicrophone =
    settings?.selected_microphone || t("common.default");
  const selectedOutput =
    settings?.selected_output_device || t("common.default");

  useEffect(() => {
    getDisplayVersion()
      .then(setVersion)
      .catch(() => setVersion("0.8.8"));
  }, []);

  return (
    <>
      <section className="android-section">
        <div className="android-section-header">
          <h2>{t("android.settings.general")}</h2>
        </div>
        <div className="android-settings-group">
          <div className="android-settings-row">
            <span>{t("settings.sound.microphone.title")}</span>
            <span className="android-muted">
              {audioDevices.find((device) => device.name === selectedMicrophone)
                ?.name || selectedMicrophone}
            </span>
          </div>
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
          <div className="android-settings-row">
            <span>{t("settings.sound.volume.title")}</span>
            <span className="android-muted">
              {audioFeedbackEnabled
                ? t("android.settings.percent", {
                    value: Math.round(settings?.audio_feedback_volume ?? 100),
                  })
                : t("common.disabled")}
            </span>
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
            <div className="android-settings-row">
              <span>{t("sidebar.dictionary")}</span>
              <BookOpen size={18} />
            </div>
            <div className="android-settings-row">
              <span>{t("sidebar.postProcessing")}</span>
              <Sparkles size={18} />
            </div>
            <div className="android-settings-row">
              <span>{t("settings.debug.historyLimit.title")}</span>
              <span className="android-muted">
                {settings?.history_limit ?? 0}
              </span>
            </div>
            <div className="android-settings-row">
              <span>{t("settings.sound.outputDevice.title")}</span>
              <span className="android-muted">
                {outputDevices.find((device) => device.name === selectedOutput)
                  ?.name || selectedOutput}
              </span>
            </div>
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
          <div className="android-settings-row">
            <span>{t("settings.about.sourceCode.title")}</span>
            <Volume2 size={18} />
          </div>
        </div>
      </section>
    </>
  );
}
