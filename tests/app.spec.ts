import { test, expect, type Page } from "@playwright/test";

const adaptiveProfiles = [
  {
    id: "default_clean",
    name: "Default Clean",
    description: "Clean dictation without changing meaning.",
    enabled: true,
  },
  {
    id: "email",
    name: "Email",
    description: "Email-ready dictation.",
    enabled: true,
  },
  {
    id: "mixed_multilingual",
    name: "Mixed Multilingual",
    description: "Preserve mixed-language intent.",
    enabled: true,
  },
];

const models = [
  {
    id: "small",
    name: "Small",
    description: "Small local model",
    filename: "small.bin",
    url: null,
    sha256: null,
    size_mb: 0,
    is_downloaded: true,
    is_downloading: false,
    partial_size: 0,
    is_directory: false,
    engine_type: "Whisper",
    accuracy_score: 3,
    speed_score: 3,
    supports_translation: false,
    is_recommended: true,
    supported_languages: ["en", "ar"],
    supports_language_selection: true,
    is_custom: false,
  },
];

const localLlmModels = [
  {
    id: "qwen2_5-0_5b-instruct-q4_k_m",
    label: "Qwen2.5 0.5B Instruct Q4_K_M",
    filename: "qwen2.5-0.5b-instruct-q4_k_m.gguf",
    url: "https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF/resolve/main/qwen2.5-0.5b-instruct-q4_k_m.gguf",
    sha256: "74a4da8c9fdbcd15bd1f6d01d621410d31c6fc00986f5eb687824e7b93d7a9db",
    size_mb: 469,
    quantization: "Q4_K_M",
    context_window: 32768,
    recommended_role: "experimental",
    supported_language_notes: "Tiny CPU fallback candidate.",
    license_label: "Apache-2.0",
    runtime: "llama.cpp",
    is_downloaded: false,
    is_downloading: false,
    partial_size: 0,
  },
];

const defaultBindings = {
  transcribe: {
    id: "transcribe",
    name: "Transcribe",
    description: "Converts your speech into text.",
    default_binding: "ctrl+space",
    current_binding: "ctrl+space",
  },
  cancel: {
    id: "cancel",
    name: "Cancel",
    description: "Cancels the current recording.",
    default_binding: "escape",
    current_binding: "escape",
  },
  transform_polish: {
    id: "transform_polish",
    name: "Polish Selected Text",
    description:
      "Transforms the selected text with the configured post-processing provider.",
    default_binding: "",
    current_binding: "",
  },
  transform_make_concise: {
    id: "transform_make_concise",
    name: "Make Selected Text Concise",
    description:
      "Makes the selected text more concise with the configured post-processing provider.",
    default_binding: "",
    current_binding: "",
  },
  transform_turn_into_list: {
    id: "transform_turn_into_list",
    name: "Turn Selected Text Into List",
    description:
      "Turns the selected text into a list with the configured post-processing provider.",
    default_binding: "",
    current_binding: "",
  },
  transform_translate: {
    id: "transform_translate",
    name: "Translate Selected Text",
    description:
      "Translates the selected text to your configured translation target.",
    default_binding: "",
    current_binding: "",
  },
  transform_prompt_engineer: {
    id: "transform_prompt_engineer",
    name: "Prompt Engineer Selected Text",
    description: "Rewrites the selected text as a clearer prompt.",
    default_binding: "",
    current_binding: "",
  },
};

const baseSettings = {
  bindings: {},
  push_to_talk: false,
  audio_feedback: false,
  audio_feedback_volume: 0.5,
  start_hidden: false,
  autostart_enabled: false,
  update_checks_enabled: false,
  selected_model: "",
  always_on_microphone: false,
  selected_microphone: "Default",
  clamshell_microphone: "Default",
  selected_output_device: "Default",
  translate_to_english: false,
  selected_language: "auto",
  dictation_language_mode: "auto",
  docked_pill_enabled: false,
  overlay_position: "top-right",
  debug_mode: false,
  log_level: "info",
  custom_words: [],
  dictionary_entries: [],
  snippets: [],
  model_unload_timeout: "never",
  word_correction_threshold: 0.8,
  history_enabled: true,
  history_limit: 100,
  recordings_enabled: true,
  recording_retention_period: "never",
  paste_method: "auto",
  clipboard_handling: "restore",
  auto_submit: false,
  auto_submit_key: "enter",
  post_process_enabled: false,
  formatting_level: "light",
  post_process_provider_id: "openai",
  post_process_providers: [],
  post_process_api_keys: {},
  post_process_models: {},
  post_process_prompts: [],
  post_process_selected_prompt_id: null,
  local_llm: {
    enabled: false,
    selected_model_id: "",
    runtime_mode: "managed",
    runtime_host: "127.0.0.1",
    runtime_port: 0,
    unload_timeout_secs: 300,
    max_output_tokens: 512,
  },
  mute_while_recording: false,
  append_trailing_space: false,
  app_language: "en",
  experimental_enabled: false,
  lazy_stream_close: false,
  keyboard_implementation: "auto",
  show_tray_icon: true,
  paste_delay_ms: 0,
  typing_tool: "auto",
  external_script_path: null,
  custom_filler_words: null,
  adaptive_profiles_enabled: false,
  context_awareness_enabled: false,
  context_nearby_text_enabled: false,
  adaptive_language_shortlist: ["fr", "de", "ja"],
  adaptive_default_profile_id: "default_clean",
  adaptive_profiles: adaptiveProfiles,
  adaptive_correction_memory_enabled: true,
  adaptive_private_app_patterns: [],
  whisper_accelerator: "auto",
  ort_accelerator: "auto",
  whisper_gpu_device: 0,
  extra_recording_buffer_ms: 0,
};

const settingRow = (page: Page, name: string) =>
  page
    .getByText(name)
    .locator("xpath=ancestor::div[contains(@class, 'justify-between')][1]");

const expectTextFits = async (page: Page, selector: string) => {
  await expect
    .poll(async () =>
      page.locator(selector).evaluate((element) => {
        const htmlElement = element as HTMLElement;
        return htmlElement.scrollWidth <= htmlElement.clientWidth;
      }),
    )
    .toBe(true);
};

const installTauriMocks = async (
  page: Page,
  settingsOverrides: Partial<typeof baseSettings> = {},
  historyEntries: Array<Record<string, unknown>> = [],
  osType: "windows" | "linux" | "macos" | "android" = "windows",
  mockOptions: {
    availableModels?: typeof models;
    currentModel?: string;
    hasAnyModels?: boolean;
    firstRun?: boolean;
    androidAsrPacks?: Array<Record<string, unknown>>;
    androidLlmPacks?: Array<Record<string, unknown>>;
    learnCandidates?: Array<Record<string, unknown>>;
    dictionaryDiagnostics?: Record<string, unknown> | null;
  } = {},
) => {
  const availableModels = mockOptions.availableModels ?? models;
  const currentModel = mockOptions.currentModel ?? "small";
  const hasAnyModelsOverride = mockOptions.hasAnyModels;
  const firstRun = Boolean(mockOptions.firstRun);
  const androidAsrPacks = mockOptions.androidAsrPacks ?? [
    {
      id: "g3-zipformer-whisper-tiny-en",
      displayName: "English On-device Starter",
      description: "Streaming Zipformer + Whisper tiny.en + Silero VAD",
      sizeMb: 141,
      installedDir:
        "/data/user/0/com.galaxyruler.verbatim/models/android-asr/g3-zipformer-whisper-tiny-en",
      isInstalled: false,
      isDownloading: false,
      isActive: false,
      isSelectable: false,
      downloadPhase: "available",
      downloadProgress: 0,
      missingFiles: [
        "streaming/encoder.onnx",
        "streaming/decoder.onnx",
        "streaming/joiner.onnx",
        "streaming/tokens.txt",
        "whisper/encoder.onnx",
        "whisper/decoder.onnx",
        "whisper/tokens.txt",
        "silero_vad_v4.onnx",
      ],
    },
  ];
  const androidLlmPacks = mockOptions.androidLlmPacks ?? [
    {
      id: "g4-qwen2_5-0_5b-litert-q8",
      displayName: "Qwen2.5 cleanup 0.5B",
      description: "LiteRT-LM cleanup model for punctuation and capitalization",
      runtime: "LiteRT-LM 0.13.1",
      license: "Apache-2.0",
      quantization: "q8 ekv1280",
      sizeMb: 522,
      minRamMb: 8192,
      installedDir:
        "/data/user/0/com.galaxyruler.verbatim/models/android-llm-postproc/g4-qwen2_5-0_5b-litert-q8",
      modelPath:
        "/data/user/0/com.galaxyruler.verbatim/models/android-llm-postproc/g4-qwen2_5-0_5b-litert-q8/qwen2.5-0.5b-instruct-q8.task",
      isInstalled: false,
      isDownloading: false,
      isActive: false,
      isSelectable: false,
      downloadPhase: "available",
      downloadProgress: 0,
      missingFiles: ["qwen2.5-0.5b-instruct-q8.task"],
    },
  ];

  await page.addInitScript(
    ({
      settings,
      profiles,
      availableModels,
      currentModel,
      hasAnyModelsOverride,
      firstRun,
      androidAsrPacks,
      androidLlmPacks,
      learnCandidates,
      dictionaryDiagnostics,
      localPostProcessingModels,
      initialHistoryEntries,
      osType,
    }) => {
      let appSettings = { ...settings };
      const callbacks = new Map<number, (payload?: unknown) => void>();
      const eventListeners = new Map<string, number[]>();
      let nextCallbackId = 1;
      const testWindow = window as typeof window & {
        __TAURI_INTERNALS__: any;
        __TAURI_EVENT_PLUGIN_INTERNALS__: any;
        __TAURI_OS_PLUGIN_INTERNALS__: any;
        __VERBATIM_TEST_COMMANDS__: string[];
        __VERBATIM_TEST_INVOKES__: Array<{
          cmd: string;
          args?: Record<string, unknown>;
        }>;
        __VERBATIM_TEST_EMITTED_EVENTS__: string[];
        __VERBATIM_TEST_LEARN_ENTRIES__: (
          entries: Array<Record<string, unknown>>,
        ) => void;
        __VERBATIM_TEST_LEARN_WORDS__: (words: string[]) => void;
        __VERBATIM_TEST_EMIT_EVENT__: (
          event: string,
          payload?: unknown,
        ) => void;
      };
      testWindow.__VERBATIM_TEST_COMMANDS__ = [];
      testWindow.__VERBATIM_TEST_INVOKES__ = [];
      testWindow.__VERBATIM_TEST_EMITTED_EVENTS__ = [];
      let dictionaryEntries = [
        ...((appSettings.dictionary_entries as Array<
          Record<string, unknown>
        >) ?? []),
      ];
      let snippetEntries = [
        ...((appSettings.snippets as Array<Record<string, unknown>>) ?? []),
      ];
      let localModels = [
        ...((localPostProcessingModels as Array<Record<string, unknown>>) ??
          []),
      ];
      let modelRows = (availableModels as Array<Record<string, unknown>>).map(
        (model) =>
          firstRun
            ? {
                ...model,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
              }
            : { ...model },
      );
      let androidAsrRows = [
        ...((androidAsrPacks as Array<Record<string, unknown>>) ?? []),
      ];
      let androidLlmRows = [
        ...((androidLlmPacks as Array<Record<string, unknown>>) ?? []),
      ];
      let historyRows = [
        ...((initialHistoryEntries as Array<Record<string, unknown>>) ?? []),
      ];
      let learnCandidateRows = [
        ...((learnCandidates as Array<Record<string, unknown>>) ?? []),
      ];
      let dictionaryDiagnosticsRow =
        (dictionaryDiagnostics as Record<string, unknown> | null) ?? null;
      const availableMicrophones = [
        { index: "default", name: "Default", is_default: true },
        { index: "studio", name: "Studio Mic", is_default: false },
      ];
      const availableOutputDevices = [
        { index: "default", name: "Default", is_default: true },
        { index: "headphones", name: "Headphones", is_default: false },
      ];
      let nextDictionaryId = 1;
      let nextSnippetId = 1;
      let nextPromptId = 1;
      const syncDictionarySettings = () => {
        appSettings = {
          ...appSettings,
          dictionary_entries: dictionaryEntries,
          custom_words: dictionaryEntries.map((entry) => entry.phrase),
        };
      };
      const syncSnippetSettings = () => {
        appSettings = {
          ...appSettings,
          snippets: snippetEntries,
        };
      };
      const emitEvent = (event: string, payload: unknown) => {
        for (const callbackId of eventListeners.get(event) ?? []) {
          callbacks.get(callbackId)?.({
            event,
            id: callbackId,
            payload,
          });
        }
      };
      testWindow.__VERBATIM_TEST_EMIT_EVENT__ = emitEvent;
      testWindow.__VERBATIM_TEST_LEARN_ENTRIES__ = (entries) => {
        dictionaryEntries = [...dictionaryEntries, ...entries];
        syncDictionarySettings();
        emitEvent("dictionary-entries-learned", entries);
      };
      testWindow.__VERBATIM_TEST_LEARN_WORDS__ = (words: string[]) => {
        const entries = words.map((word) => ({
          id: `dict_test_${nextDictionaryId++}`,
          phrase: word,
          replacement_of: null,
          source: "auto_learned",
          priority: "normal",
          created_at_ms: 1,
          updated_at_ms: 1,
        }));
        testWindow.__VERBATIM_TEST_LEARN_ENTRIES__(entries);
      };

      testWindow.__TAURI_OS_PLUGIN_INTERNALS__ = {
        platform: osType,
        os_type: osType,
        family: osType,
        version: "11",
        arch: "x86_64",
        exe_extension: osType === "windows" ? "exe" : "",
        eol: osType === "windows" ? "\r\n" : "\n",
      };
      testWindow.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
        unregisterListener: (event: string, eventId: number) => {
          eventListeners.set(
            event,
            (eventListeners.get(event) ?? []).filter((id) => id !== eventId),
          );
          callbacks.delete(eventId);
        },
      };

      testWindow.__TAURI_INTERNALS__ = {
        callbacks: {},
        convertFileSrc: (filePath: string) => filePath,
        invoke: async (cmd: string, args?: Record<string, unknown>) => {
          testWindow.__VERBATIM_TEST_COMMANDS__.push(cmd);
          testWindow.__VERBATIM_TEST_INVOKES__.push({ cmd, args });
          if (cmd === "plugin:event|listen") {
            const event = args?.event as string;
            const handler = args?.handler as number;
            eventListeners.set(event, [
              ...(eventListeners.get(event) ?? []),
              handler,
            ]);
            return handler;
          }
          if (cmd === "plugin:event|emit") {
            const event = args?.event as string;
            testWindow.__VERBATIM_TEST_EMITTED_EVENTS__.push(event);
            emitEvent(event, args?.payload);
            return null;
          }
          if (cmd === "plugin:event|unlisten") {
            const event = args?.event as string;
            const eventId = args?.eventId as number;
            eventListeners.set(
              event,
              (eventListeners.get(event) ?? []).filter((id) => id !== eventId),
            );
            return null;
          }
          // verbatim-android plugin: serve from the same window.VerbatimAndroid mock so the
          // tests exercise the real adapter (src/android/bridge.ts) plugin path.
          if (cmd.startsWith("plugin:verbatim-android|")) {
            const va = (
              window as unknown as {
                VerbatimAndroid?: {
                  permissionSnapshot: () => string;
                  nativeTranscriptHistory: () => string;
                  bubbleCornerSnapshot: () => string;
                  setBubbleCorner: (corner: string) => string;
                  openExternalUrl: (url: string) => boolean;
                  requestMicrophone: () => void;
                  openOverlaySettings: () => void;
                  openAccessibilitySettings: () => void;
                  requestSpeechModelDownload: () => void;
                  syncTextFormatter: (snapshot: string) => void;
                  engineDictationEnabled?: () => boolean;
                  setEngineDictationEnabled?: (enabled: boolean) => boolean;
                  setEngineModelId?: (modelId: string) => string;
                  llmPostProcessingSupport?: () => Record<string, unknown>;
                  llmPostProcessingEnabled?: () => boolean;
                  setLlmPostProcessingEnabled?: (enabled: boolean) => boolean;
                  setLlmModelId?: (modelId: string) => string;
                  startBubble: () => void;
                  stopBubble: () => void;
                };
              }
            ).VerbatimAndroid;
            const method = cmd.slice("plugin:verbatim-android|".length);
            switch (method) {
              case "permission_snapshot":
                return va ? JSON.parse(va.permissionSnapshot()) : {};
              case "native_transcript_history":
                return { json: va?.nativeTranscriptHistory() ?? "[]" };
              case "sync_text_formatter":
                va?.syncTextFormatter(args?.snapshot as string);
                return null;
              case "bubble_corner_snapshot":
                return { value: va?.bubbleCornerSnapshot() };
              case "set_bubble_corner":
                return { value: va?.setBubbleCorner(args?.corner as string) };
              case "open_external_url":
                return {
                  value: va?.openExternalUrl(args?.url as string) ?? false,
                };
              case "request_microphone":
                va?.requestMicrophone();
                return null;
              case "open_overlay_settings":
                va?.openOverlaySettings();
                return null;
              case "open_accessibility_settings":
                va?.openAccessibilitySettings();
                return null;
              case "request_speech_model_download":
                va?.requestSpeechModelDownload();
                return null;
              case "engine_dictation_enabled":
                return { value: va?.engineDictationEnabled?.() ?? false };
              case "set_engine_dictation_enabled":
                return {
                  value:
                    va?.setEngineDictationEnabled?.(args?.enabled as boolean) ??
                    false,
                };
              case "set_engine_model_id":
                return {
                  value:
                    va?.setEngineModelId?.(args?.modelId as string) ??
                    (args?.modelId as string),
                };
              case "llm_post_processing_support":
                return va?.llmPostProcessingSupport?.() ?? {};
              case "llm_post_processing_enabled":
                return { value: va?.llmPostProcessingEnabled?.() ?? false };
              case "set_llm_post_processing_enabled":
                return {
                  value:
                    va?.setLlmPostProcessingEnabled?.(
                      args?.enabled as boolean,
                    ) ?? false,
                };
              case "set_llm_model_id":
                return {
                  value:
                    va?.setLlmModelId?.(args?.modelId as string) ??
                    (args?.modelId as string),
                };
              case "start_bubble":
                va?.startBubble();
                return null;
              case "stop_bubble":
                va?.stopBubble();
                return null;
              default:
                // registerListener / removeListener etc. — no-op resolve.
                return null;
            }
          }
          switch (cmd) {
            case "get_startup_status":
              return { status: "ready" };
            case "get_default_settings":
            case "get_app_settings":
              syncDictionarySettings();
              syncSnippetSettings();
              return appSettings;
            case "list_dictionary_entries":
              return dictionaryEntries;
            case "add_dictionary_entry": {
              const input = args?.input as {
                phrase: string;
                replacement_of?: string | null;
              };
              const entry = {
                id: `dict_test_${nextDictionaryId++}`,
                phrase: input.phrase,
                replacement_of: input.replacement_of ?? null,
                source: "manual",
                priority: "normal",
                created_at_ms: nextDictionaryId,
                updated_at_ms: nextDictionaryId,
              };
              dictionaryEntries = [...dictionaryEntries, entry];
              syncDictionarySettings();
              return entry;
            }
            case "update_dictionary_entry": {
              const id = args?.id as string;
              const update = args?.update as {
                phrase?: string | null;
                replacement_of?: string | null;
                priority?: "normal" | "starred" | null;
              };
              let updated = null;
              dictionaryEntries = dictionaryEntries.map((entry) => {
                if (entry.id !== id) return entry;
                updated = {
                  ...entry,
                  phrase: update.phrase ?? entry.phrase,
                  replacement_of:
                    "replacement_of" in update
                      ? update.replacement_of
                      : entry.replacement_of,
                  priority: update.priority ?? entry.priority,
                  updated_at_ms: Date.now(),
                };
                return updated;
              });
              syncDictionarySettings();
              return updated;
            }
            case "delete_dictionary_entry":
              dictionaryEntries = dictionaryEntries.filter(
                (entry) => entry.id !== args?.id,
              );
              syncDictionarySettings();
              return null;
            case "undo_dictionary_entries": {
              const ids = new Set(args?.ids as string[]);
              const deleted = dictionaryEntries.filter((entry) =>
                ids.has(entry.id as string),
              );
              dictionaryEntries = dictionaryEntries.filter(
                (entry) => !ids.has(entry.id as string),
              );
              syncDictionarySettings();
              return deleted;
            }
            case "set_dictionary_entry_active": {
              const id = args?.id as string;
              const active = Boolean(args?.active);
              dictionaryEntries = dictionaryEntries.map((entry) =>
                entry.id === id
                  ? { ...entry, active, needs_review: false }
                  : entry,
              );
              syncDictionarySettings();
              return null;
            }
            case "list_learn_candidates":
              return learnCandidateRows;
            case "approve_learn_candidate": {
              const phrase = args?.phrase as string;
              const replacementOf = (args?.replacementOf ?? null) as
                | string
                | null;
              learnCandidateRows = learnCandidateRows.filter(
                (candidate) => candidate.phrase !== phrase,
              );
              const entry = {
                id: `dict_test_${nextDictionaryId++}`,
                phrase,
                replacement_of: replacementOf,
                source: "auto_learned",
                priority: "normal",
                created_at_ms: Date.now(),
                updated_at_ms: Date.now(),
                active: true,
                user_confirmed: true,
                needs_review: false,
              };
              dictionaryEntries = [...dictionaryEntries, entry];
              syncDictionarySettings();
              return entry;
            }
            case "reject_learn_candidate": {
              const phrase = args?.phrase as string;
              learnCandidateRows = learnCandidateRows.filter(
                (candidate) => candidate.phrase !== phrase,
              );
              return null;
            }
            case "get_dictionary_diagnostics":
              return dictionaryDiagnosticsRow;
            case "reset_dictionary_diagnostics": {
              dictionaryDiagnosticsRow = null;
              return null;
            }
            case "list_snippet_entries":
              return snippetEntries;
            case "add_snippet_entry": {
              const input = args?.input as {
                trigger: string;
                content: string;
              };
              const entry = {
                id: `snippet_test_${nextSnippetId++}`,
                trigger: input.trigger,
                content: input.content,
                created_at_ms: nextSnippetId,
                updated_at_ms: nextSnippetId,
              };
              snippetEntries = [...snippetEntries, entry];
              syncSnippetSettings();
              return entry;
            }
            case "update_snippet_entry": {
              const id = args?.id as string;
              const update = args?.update as {
                trigger?: string | null;
                content?: string | null;
              };
              let updated = null;
              snippetEntries = snippetEntries.map((entry) => {
                if (entry.id !== id) return entry;
                updated = {
                  ...entry,
                  trigger: update.trigger ?? entry.trigger,
                  content: update.content ?? entry.content,
                  updated_at_ms: Date.now(),
                };
                return updated;
              });
              syncSnippetSettings();
              return updated;
            }
            case "delete_snippet_entry":
              snippetEntries = snippetEntries.filter(
                (entry) => entry.id !== args?.id,
              );
              syncSnippetSettings();
              return null;
            case "copy_last_transcript":
            case "copy_last_transform_result":
            case "paste_last_transcript":
              return true;
            case "get_history_entries": {
              const cursor = args?.cursor as number | null | undefined;
              const limit = (args?.limit as number | null | undefined) ?? 50;
              const rows =
                cursor === null || cursor === undefined
                  ? historyRows
                  : historyRows.filter((entry) => Number(entry.id) < cursor);
              return {
                entries: rows.slice(0, limit),
                has_more: rows.length > limit,
              };
            }
            case "toggle_history_entry_saved": {
              const id = Number(args?.id);
              historyRows = historyRows.map((entry) =>
                Number(entry.id) === id
                  ? { ...entry, saved: !entry.saved }
                  : entry,
              );
              return null;
            }
            case "get_audio_file_path":
              return "mock-audio.wav";
            case "delete_history_entry":
              historyRows = historyRows.filter(
                (entry) => Number(entry.id) !== Number(args?.id),
              );
              return null;
            case "retry_history_entry_transcription":
              return null;
            case "learn_custom_words_from_correction":
              return ["CorrectedName"];
            case "list_local_llm_models":
              return localModels;
            case "download_local_llm_model": {
              const modelId = args?.modelId as string;
              localModels = localModels.map((model) =>
                model.id === modelId
                  ? {
                      ...model,
                      is_downloaded: true,
                      is_downloading: false,
                      partial_size: 0,
                    }
                  : model,
              );
              emitEvent("local-llm-model-changed", modelId);
              return null;
            }
            case "cancel_local_llm_download":
              return null;
            case "delete_local_llm_model": {
              const modelId = args?.modelId as string;
              localModels = localModels.map((model) =>
                model.id === modelId
                  ? {
                      ...model,
                      is_downloaded: false,
                      is_downloading: false,
                      partial_size: 0,
                    }
                  : model,
              );
              if (appSettings.local_llm.selected_model_id === modelId) {
                appSettings = {
                  ...appSettings,
                  local_llm: {
                    ...appSettings.local_llm,
                    enabled: false,
                    selected_model_id: "",
                  },
                };
              }
              emitEvent("local-llm-model-changed", modelId);
              return null;
            }
            case "select_local_llm_model": {
              const modelId = args?.modelId as string;
              appSettings = {
                ...appSettings,
                local_llm: {
                  ...appSettings.local_llm,
                  selected_model_id: modelId,
                },
              };
              return null;
            }
            case "set_local_llm_enabled":
              appSettings = {
                ...appSettings,
                local_llm: {
                  ...appSettings.local_llm,
                  enabled: Boolean(args?.enabled),
                },
              };
              return null;
            case "has_any_models_available":
              return (
                hasAnyModelsOverride ??
                modelRows.some((model) => Boolean(model.is_downloaded))
              );
            case "get_available_models":
              return modelRows;
            case "asr_list_model_packs":
              return androidAsrRows;
            case "asr_download_model_pack": {
              const modelId = args?.modelId as string;
              androidAsrRows = androidAsrRows.map((model) =>
                model.id === modelId
                  ? {
                      ...model,
                      isDownloading: true,
                      downloadPhase: "downloading",
                      downloadProgress: 42,
                    }
                  : model,
              );
              emitEvent("android-asr-model-progress", {
                modelId,
                phase: "downloading",
                downloaded: 42,
                total: 100,
                percentage: 42,
              });

              return new Promise((resolve) => {
                setTimeout(() => {
                  androidAsrRows = androidAsrRows.map((model) =>
                    model.id === modelId
                      ? {
                          ...model,
                          isDownloading: true,
                          downloadPhase: "verifying",
                          downloadProgress: 100,
                        }
                      : model,
                  );
                  emitEvent("android-asr-model-progress", {
                    modelId,
                    phase: "verifying",
                    downloaded: 100,
                    total: 100,
                    percentage: 100,
                  });
                }, 100);
                setTimeout(() => {
                  androidAsrRows = androidAsrRows.map((model) =>
                    model.id === modelId
                      ? {
                          ...model,
                          isInstalled: true,
                          isDownloading: false,
                          isSelectable: true,
                          downloadPhase: "ready",
                          downloadProgress: 100,
                          missingFiles: [],
                        }
                      : model,
                  );
                  emitEvent("android-asr-model-changed", modelId);
                  resolve(null);
                }, 250);
              });
            }
            case "asr_cancel_model_download":
              androidAsrRows = androidAsrRows.map((model) =>
                model.id === args?.modelId
                  ? {
                      ...model,
                      isDownloading: false,
                      downloadPhase: "available",
                      downloadProgress: 0,
                    }
                  : model,
              );
              return null;
            case "asr_select_model_pack": {
              const modelId = args?.modelId as string;
              androidAsrRows = androidAsrRows.map((model) => ({
                ...model,
                isActive: model.id === modelId,
              }));
              return androidAsrRows.find((model) => model.id === modelId);
            }
            case "asr_delete_model_pack": {
              const modelId = args?.modelId as string;
              androidAsrRows = androidAsrRows.map((model) =>
                model.id === modelId
                  ? {
                      ...model,
                      isInstalled: false,
                      isDownloading: false,
                      isActive: false,
                      isSelectable: false,
                      downloadPhase: "available",
                      downloadProgress: 0,
                      missingFiles: ["streaming/encoder.onnx"],
                    }
                  : model,
              );
              emitEvent("android-asr-model-changed", modelId);
              return null;
            }
            case "llm_list_model_packs":
              return androidLlmRows;
            case "llm_download_model_pack": {
              const modelId = args?.modelId as string;
              androidLlmRows = androidLlmRows.map((model) =>
                model.id === modelId
                  ? {
                      ...model,
                      isDownloading: true,
                      downloadPhase: "downloading",
                      downloadProgress: 42,
                    }
                  : model,
              );
              emitEvent("android-llm-model-progress", {
                modelId,
                phase: "downloading",
                downloaded: 42,
                total: 100,
                percentage: 42,
              });

              return new Promise((resolve) => {
                setTimeout(() => {
                  androidLlmRows = androidLlmRows.map((model) =>
                    model.id === modelId
                      ? {
                          ...model,
                          isDownloading: true,
                          downloadPhase: "verifying",
                          downloadProgress: 100,
                        }
                      : model,
                  );
                  emitEvent("android-llm-model-progress", {
                    modelId,
                    phase: "verifying",
                    downloaded: 100,
                    total: 100,
                    percentage: 100,
                  });
                }, 100);
                setTimeout(() => {
                  androidLlmRows = androidLlmRows.map((model) =>
                    model.id === modelId
                      ? {
                          ...model,
                          isInstalled: true,
                          isDownloading: false,
                          isSelectable: true,
                          downloadPhase: "ready",
                          downloadProgress: 100,
                          missingFiles: [],
                        }
                      : model,
                  );
                  emitEvent("android-llm-model-changed", modelId);
                  resolve(null);
                }, 250);
              });
            }
            case "llm_cancel_model_download":
              androidLlmRows = androidLlmRows.map((model) =>
                model.id === args?.modelId
                  ? {
                      ...model,
                      isDownloading: false,
                      downloadPhase: "available",
                      downloadProgress: 0,
                    }
                  : model,
              );
              return null;
            case "llm_select_model_pack": {
              const modelId = args?.modelId as string;
              androidLlmRows = androidLlmRows.map((model) => ({
                ...model,
                isActive: model.id === modelId,
              }));
              return androidLlmRows.find((model) => model.id === modelId);
            }
            case "llm_delete_model_pack": {
              const modelId = args?.modelId as string;
              androidLlmRows = androidLlmRows.map((model) =>
                model.id === modelId
                  ? {
                      ...model,
                      isInstalled: false,
                      isDownloading: false,
                      isActive: false,
                      isSelectable: false,
                      downloadPhase: "available",
                      downloadProgress: 0,
                      missingFiles: ["qwen2.5-0.5b-instruct-q8.task"],
                    }
                  : model,
              );
              emitEvent("android-llm-model-changed", modelId);
              return null;
            }
            case "download_model": {
              const modelId = args?.modelId as string;
              modelRows = modelRows.map((model) =>
                model.id === modelId
                  ? {
                      ...model,
                      is_downloaded: true,
                      is_downloading: false,
                      partial_size: 0,
                    }
                  : model,
              );
              setTimeout(
                () => emitEvent("model-download-complete", modelId),
                0,
              );
              return null;
            }
            case "set_active_model":
              appSettings = {
                ...appSettings,
                selected_model: args?.modelId as string,
              };
              return null;
            case "get_current_model":
            case "get_transcription_model_status":
              return appSettings.selected_model || currentModel;
            case "get_windows_microphone_permission_status":
              return {
                supported: true,
                overall_access: "allowed",
                device_access: "allowed",
                app_access: "allowed",
                desktop_app_access: "allowed",
              };
            case "check_custom_sounds":
              return { start: false, stop: false };
            case "initialize_enigo":
            case "initialize_shortcuts":
              return null;
            case "get_available_microphones":
              return availableMicrophones;
            case "get_available_output_devices":
              return availableOutputDevices;
            case "change_audio_feedback_setting":
              appSettings = {
                ...appSettings,
                audio_feedback: Boolean(args?.enabled),
              };
              return null;
            case "change_audio_feedback_volume_setting":
              appSettings = {
                ...appSettings,
                audio_feedback_volume: Number(args?.volume),
              };
              return null;
            case "set_selected_microphone":
              appSettings = {
                ...appSettings,
                selected_microphone:
                  args?.deviceName === "default"
                    ? "Default"
                    : String(args?.deviceName ?? "Default"),
              };
              return null;
            case "set_selected_output_device":
              appSettings = {
                ...appSettings,
                selected_output_device:
                  args?.deviceName === "default"
                    ? "Default"
                    : String(args?.deviceName ?? "Default"),
              };
              return null;
            case "change_sound_theme_setting":
              appSettings = {
                ...appSettings,
                sound_theme: String(args?.theme ?? "marimba"),
              };
              return null;
            case "change_app_language_setting":
              appSettings = {
                ...appSettings,
                app_language: String(args?.language ?? "en"),
              };
              return null;
            case "update_history_limit":
              appSettings = {
                ...appSettings,
                history_limit: Number(args?.limit),
              };
              return null;
            case "update_recording_retention_period":
              appSettings = {
                ...appSettings,
                recording_retention_period: String(args?.period ?? "never"),
              };
              return null;
            case "start_microphone_test":
              setTimeout(
                () =>
                  emitEvent(
                    "mic-level",
                    [
                      0.02, 0.04, 0.12, 0.22, 0.35, 0.48, 0.38, 0.25, 0.14,
                      0.08, 0.04, 0.02, 0.01, 0, 0, 0,
                    ],
                  ),
                0,
              );
              return {
                selected_microphone:
                  appSettings.selected_microphone ?? "default",
                stream_open: true,
              };
            case "stop_microphone_test":
              return true;
            case "start_onboarding_dictation_test":
              return true;
            case "stop_onboarding_dictation_test":
              return {
                text: "Testing Verbatim setup.",
                captured_sample_count: 32000,
                observed_active_signal: true,
              };
            case "cancel_onboarding_dictation_test":
            case "copy_onboarding_dictation_text":
              return true;
            case "get_adaptive_profiles":
              return profiles;
            case "change_experimental_enabled_setting":
              appSettings = {
                ...appSettings,
                experimental_enabled: Boolean(args?.enabled),
              };
              return null;
            case "change_formatting_level_setting":
              appSettings = {
                ...appSettings,
                formatting_level: args?.level as string,
              };
              return null;
            case "change_post_process_enabled_setting":
              appSettings = {
                ...appSettings,
                post_process_enabled: Boolean(args?.enabled),
              };
              return null;
            case "set_post_process_provider":
              appSettings = {
                ...appSettings,
                post_process_provider_id: String(args?.providerId ?? ""),
              };
              return null;
            case "change_post_process_base_url_setting": {
              const providerId = String(args?.providerId ?? "");
              const baseUrl = String(args?.baseUrl ?? "");
              appSettings = {
                ...appSettings,
                post_process_providers: (
                  (appSettings.post_process_providers as Array<
                    Record<string, unknown>
                  >) ?? []
                ).map((provider) =>
                  provider.id === providerId
                    ? { ...provider, base_url: baseUrl }
                    : provider,
                ),
                post_process_models: {
                  ...((appSettings.post_process_models as Record<
                    string,
                    string
                  >) ?? {}),
                  [providerId]: "",
                },
              };
              return null;
            }
            case "change_post_process_api_key_setting": {
              const providerId = String(args?.providerId ?? "");
              appSettings = {
                ...appSettings,
                post_process_api_keys: {
                  ...((appSettings.post_process_api_keys as Record<
                    string,
                    string
                  >) ?? {}),
                  [providerId]: String(args?.apiKey ?? ""),
                },
              };
              return null;
            }
            case "change_post_process_model_setting": {
              const providerId = String(args?.providerId ?? "");
              appSettings = {
                ...appSettings,
                post_process_models: {
                  ...((appSettings.post_process_models as Record<
                    string,
                    string
                  >) ?? {}),
                  [providerId]: String(args?.model ?? ""),
                },
              };
              return null;
            }
            case "fetch_post_process_models": {
              const providerId = String(args?.providerId ?? "");
              return providerId === "anthropic"
                ? ["claude-3-5-haiku-latest", "claude-3-5-sonnet-latest"]
                : ["gpt-4o-mini", "gpt-4.1-mini"];
            }
            case "set_post_process_selected_prompt":
              appSettings = {
                ...appSettings,
                post_process_selected_prompt_id: String(args?.id ?? ""),
              };
              return null;
            case "add_post_process_prompt": {
              const prompt = {
                id: `prompt_test_${nextPromptId++}`,
                name: String(args?.name ?? ""),
                prompt: String(args?.prompt ?? ""),
              };
              appSettings = {
                ...appSettings,
                post_process_prompts: [
                  ...((appSettings.post_process_prompts as Array<
                    Record<string, unknown>
                  >) ?? []),
                  prompt,
                ],
              };
              return prompt;
            }
            case "update_post_process_prompt": {
              const id = String(args?.id ?? "");
              appSettings = {
                ...appSettings,
                post_process_prompts: (
                  (appSettings.post_process_prompts as Array<
                    Record<string, unknown>
                  >) ?? []
                ).map((prompt) =>
                  prompt.id === id
                    ? {
                        ...prompt,
                        name: String(args?.name ?? ""),
                        prompt: String(args?.prompt ?? ""),
                      }
                    : prompt,
                ),
              };
              return null;
            }
            case "delete_post_process_prompt": {
              const id = String(args?.id ?? "");
              appSettings = {
                ...appSettings,
                post_process_prompts: (
                  (appSettings.post_process_prompts as Array<
                    Record<string, unknown>
                  >) ?? []
                ).filter((prompt) => prompt.id !== id),
              };
              return null;
            }
            case "change_adaptive_profiles_enabled_setting":
              appSettings = {
                ...appSettings,
                adaptive_profiles_enabled: Boolean(args?.enabled),
              };
              return null;
            case "change_context_awareness_enabled_setting":
              appSettings = {
                ...appSettings,
                context_awareness_enabled: Boolean(args?.enabled),
                context_nearby_text_enabled: args?.enabled
                  ? appSettings.context_nearby_text_enabled
                  : false,
              };
              return null;
            case "change_context_nearby_text_enabled_setting":
              appSettings = {
                ...appSettings,
                context_nearby_text_enabled:
                  appSettings.context_awareness_enabled &&
                  Boolean(args?.enabled),
              };
              return null;
            case "change_dictation_language_mode_setting": {
              const mode = args?.mode as string;
              const selectedLanguage = args?.selectedLanguage as
                | string
                | undefined;
              const languages = (args?.languages as string[]) ?? [
                "fr",
                "de",
                "ja",
              ];
              const languageSettings = {
                selected_language:
                  mode === "single"
                    ? (selectedLanguage ?? languages[0] ?? "en")
                    : "auto",
                adaptive_language_shortlist: languages,
              };
              appSettings = {
                ...appSettings,
                dictation_language_mode: mode,
                ...languageSettings,
              };
              return null;
            }
            case "change_docked_pill_setting": {
              appSettings = {
                ...appSettings,
                docked_pill_enabled: Boolean(args?.enabled),
              };
              return null;
            }
            default:
              return null;
          }
        },
        transformCallback: (callback: (payload?: unknown) => void) => {
          const id = nextCallbackId++;
          callbacks.set(id, callback);
          return id;
        },
        unregisterCallback: (id: number) => {
          callbacks.delete(id);
        },
        runCallback: (id: number, payload?: unknown) => {
          callbacks.get(id)?.(payload);
        },
        metadata: {
          currentWindow: { label: "main" },
          currentWebview: { label: "main" },
        },
      };
    },
    {
      settings: { ...baseSettings, ...settingsOverrides },
      profiles: adaptiveProfiles,
      availableModels,
      currentModel,
      hasAnyModelsOverride,
      firstRun,
      androidAsrPacks,
      androidLlmPacks,
      learnCandidates: mockOptions.learnCandidates ?? [],
      dictionaryDiagnostics: mockOptions.dictionaryDiagnostics ?? null,
      localPostProcessingModels: localLlmModels,
      initialHistoryEntries: historyEntries,
      osType,
    },
  );
};

const installAndroidBridgeMock = async (
  page: Page,
  snapshot: {
    microphone: boolean;
    overlay: boolean;
    accessibility: boolean;
    bubbleRunning: boolean;
    bubbleVisible?: boolean;
    speechRecognizerAvailable: boolean;
    onDeviceSpeechRecognizerAvailable: boolean;
    onDeviceSpeechLanguageAvailable?: boolean;
    onDeviceSpeechModelStatus?: string;
    llmPostProcessingSupported?: boolean;
    llmPostProcessingReason?: string;
    llmTotalRamMb?: number;
    llmAvailableRamMb?: number;
    llmMinRamMb?: number;
    llmHardware?: string;
    llmSocModel?: string;
  },
  nativeHistoryEntries: Array<Record<string, unknown>> = [],
  bubbleCorner = "top-right",
) => {
  await page.addInitScript(
    ({ snapshot, nativeHistoryEntries, bubbleCorner }) => {
      let selectedBubbleCorner = bubbleCorner;
      const testWindow = window as typeof window & {
        VerbatimAndroid: {
          permissionSnapshot: () => string;
          nativeTranscriptHistory: () => string;
          bubbleCornerSnapshot: () => string;
          setBubbleCorner: (corner: string) => string;
          openExternalUrl: (url: string) => boolean;
          requestMicrophone: () => void;
          openOverlaySettings: () => void;
          openAccessibilitySettings: () => void;
          requestSpeechModelDownload: () => void;
          syncTextFormatter: (snapshot: string) => void;
          engineDictationEnabled: () => boolean;
          setEngineDictationEnabled: (enabled: boolean) => boolean;
          setEngineModelId: (modelId: string) => string;
          llmPostProcessingSupport: () => Record<string, unknown>;
          llmPostProcessingEnabled: () => boolean;
          setLlmPostProcessingEnabled: (enabled: boolean) => boolean;
          setLlmModelId: (modelId: string) => string;
          startBubble: () => void;
          stopBubble: () => void;
        };
        __VERBATIM_ANDROID_BRIDGE_CALLS__: string[];
        __VERBATIM_ANDROID_FORMATTER_SNAPSHOTS__: string[];
      };
      testWindow.__VERBATIM_ANDROID_BRIDGE_CALLS__ = [];
      testWindow.__VERBATIM_ANDROID_FORMATTER_SNAPSHOTS__ = [];
      let engineDictationEnabled = false;
      let engineModelId = "default";
      let llmCleanupEnabled = false;
      let llmModelId = "default";
      testWindow.VerbatimAndroid = {
        permissionSnapshot: () => JSON.stringify(snapshot),
        nativeTranscriptHistory: () => JSON.stringify(nativeHistoryEntries),
        bubbleCornerSnapshot: () => selectedBubbleCorner,
        setBubbleCorner: (corner: string) => {
          selectedBubbleCorner = corner;
          testWindow.__VERBATIM_ANDROID_BRIDGE_CALLS__.push(
            `setBubbleCorner:${corner}`,
          );
          return selectedBubbleCorner;
        },
        openExternalUrl: (url: string) => {
          testWindow.__VERBATIM_ANDROID_BRIDGE_CALLS__.push(
            `openExternalUrl:${url}`,
          );
          return true;
        },
        requestMicrophone: () => undefined,
        openOverlaySettings: () => undefined,
        openAccessibilitySettings: () => undefined,
        requestSpeechModelDownload: () => {
          testWindow.__VERBATIM_ANDROID_BRIDGE_CALLS__.push(
            "requestSpeechModelDownload",
          );
        },
        syncTextFormatter: (formatterSnapshot: string) => {
          testWindow.__VERBATIM_ANDROID_FORMATTER_SNAPSHOTS__.push(
            formatterSnapshot,
          );
        },
        engineDictationEnabled: () => engineDictationEnabled,
        setEngineDictationEnabled: (enabled: boolean) => {
          engineDictationEnabled = enabled;
          testWindow.__VERBATIM_ANDROID_BRIDGE_CALLS__.push(
            `setEngineDictationEnabled:${enabled}`,
          );
          return engineDictationEnabled;
        },
        setEngineModelId: (modelId: string) => {
          engineModelId = modelId;
          testWindow.__VERBATIM_ANDROID_BRIDGE_CALLS__.push(
            `setEngineModelId:${modelId}`,
          );
          return engineModelId;
        },
        llmPostProcessingSupport: () => ({
          supported: Boolean(snapshot.llmPostProcessingSupported),
          reason:
            snapshot.llmPostProcessingReason ??
            (snapshot.llmPostProcessingSupported
              ? "supported"
              : "requiresHighEndSoc"),
          totalRamMb: snapshot.llmTotalRamMb ?? 0,
          availableRamMb: snapshot.llmAvailableRamMb ?? 0,
          minRamMb: snapshot.llmMinRamMb ?? 8192,
          hardware: snapshot.llmHardware ?? "",
          socModel: snapshot.llmSocModel ?? "",
        }),
        llmPostProcessingEnabled: () =>
          llmCleanupEnabled && Boolean(snapshot.llmPostProcessingSupported),
        setLlmPostProcessingEnabled: (enabled: boolean) => {
          llmCleanupEnabled =
            enabled && Boolean(snapshot.llmPostProcessingSupported);
          testWindow.__VERBATIM_ANDROID_BRIDGE_CALLS__.push(
            `setLlmPostProcessingEnabled:${enabled}`,
          );
          return llmCleanupEnabled;
        },
        setLlmModelId: (modelId: string) => {
          llmModelId = modelId;
          testWindow.__VERBATIM_ANDROID_BRIDGE_CALLS__.push(
            `setLlmModelId:${modelId}`,
          );
          return llmModelId;
        },
        startBubble: () => undefined,
        stopBubble: () => undefined,
      };
    },
    { snapshot, nativeHistoryEntries, bubbleCorner },
  );
};

test.describe("Verbatim App", () => {
  test("dev server responds", async ({ page }) => {
    // Just verify the dev server is running and responds
    const response = await page.goto("/");
    expect(response?.status()).toBe(200);
  });

  test("page has html structure", async ({ page }) => {
    await page.goto("/");

    // Verify basic HTML structure exists
    const html = await page.content();
    expect(html).toContain("<html");
    expect(html).toContain("<body");
  });

  test("first-run onboarding verifies shortcut and microphone readiness", async ({
    page,
  }) => {
    await installTauriMocks(
      page,
      { selected_model: "", bindings: defaultBindings },
      [],
      "windows",
      { firstRun: true },
    );
    await page.goto("/");

    await expect(page.getByText("To get started")).toBeVisible();
    await page.getByRole("button", { name: /Whisper Small/ }).click();

    await expect(page.getByText("Set recording shortcut")).toBeVisible();
    await page.getByRole("button", { name: "Continue" }).click();

    await expect(page.getByText("Test microphone")).toBeVisible();
    await expect(page.getByRole("button", { name: "Default" })).toBeVisible();
    await page.getByRole("button", { name: "Start test" }).click();
    await expect(page.getByText("Microphone input detected.")).toBeVisible();
    await page.getByRole("button", { name: "Continue" }).click();

    await expect(page.getByText("Test dictation")).toBeVisible();
    await page.getByRole("button", { name: "Start recording" }).click();
    await page.getByRole("button", { name: /Stop recording/ }).click();
    await expect(page.getByText("Testing Verbatim setup.")).toBeVisible();
    await page.getByRole("button", { name: /Copy test text/ }).click();
    await expect(
      page.getByText("Test text copied to your clipboard."),
    ).toBeVisible();
    await page.getByRole("button", { name: "Discard and continue" }).click();

    await expect(page.getByTitle("General")).toBeVisible();

    const commands = await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_COMMANDS__: string[];
      };
      return win.__VERBATIM_TEST_COMMANDS__;
    });
    expect(commands).toContain("start_microphone_test");
    expect(commands).toContain("stop_microphone_test");
    expect(commands).toContain("start_onboarding_dictation_test");
    expect(commands).toContain("stop_onboarding_dictation_test");
    expect(commands).toContain("copy_onboarding_dictation_text");
  });

  test("settings sidebar sections are keyboard-operable", async ({ page }) => {
    await installTauriMocks(page, { post_process_enabled: true });
    await page.goto("/");
    await expect(page.getByTitle("General")).toBeVisible();

    for (const sectionName of [
      "General",
      "Models",
      "Dictionary",
      "Snippets",
      "Advanced",
      "History & Privacy",
      "Post-processing",
      "Troubleshooting",
      "About",
    ]) {
      const sectionButton = page.getByRole("button", {
        name: sectionName,
        exact: true,
      });

      await expect(sectionButton).toBeVisible();
      await sectionButton.press("Enter");
      await expect(sectionButton).toHaveAttribute("aria-current", "page");
    }
  });

  test("android setup requires on-device speech before bubble readiness", async ({
    page,
  }) => {
    await installTauriMocks(page, {}, [], "android");
    await installAndroidBridgeMock(page, {
      microphone: true,
      overlay: true,
      accessibility: true,
      bubbleRunning: true,
      speechRecognizerAvailable: true,
      onDeviceSpeechRecognizerAvailable: false,
    });

    await page.goto("/");

    await expect(
      page.getByRole("heading", { name: "Set up mobile dictation" }),
    ).toBeVisible();
    await expect(page.getByText("Use on-device speech")).toBeVisible();
    await expect(
      page.getByText(
        "Install an offline speech pack or use a device with on-device speech. Verbatim will not silently use remote speech.",
      ),
    ).toBeVisible();
    await expect(page.getByText("Unavailable")).toBeVisible();
    await expect(
      page.getByRole("heading", {
        name: "Tap the Verbatim bubble to dictate anywhere",
      }),
    ).toHaveCount(0);
  });

  test("android setup requires a downloaded offline speech pack before bubble readiness", async ({
    page,
  }) => {
    await installTauriMocks(page, {}, [], "android");
    await installAndroidBridgeMock(page, {
      microphone: true,
      overlay: true,
      accessibility: true,
      bubbleRunning: true,
      speechRecognizerAvailable: true,
      onDeviceSpeechRecognizerAvailable: true,
      onDeviceSpeechLanguageAvailable: false,
      onDeviceSpeechModelStatus: "missing",
    });

    await page.goto("/");

    await expect(
      page.getByRole("heading", { name: "Set up mobile dictation" }),
    ).toBeVisible();
    await expect(page.getByText("Download offline speech pack")).toBeVisible();
    await expect(
      page.getByText(
        "Install the local Android speech pack for your current language before dictation starts.",
      ),
    ).toBeVisible();
    await page.getByRole("button", { name: "Download pack" }).click();
    await expect(
      page.getByRole("heading", {
        name: "Tap the Verbatim bubble to dictate anywhere",
      }),
    ).toHaveCount(0);
    await expect
      .poll(() =>
        page.evaluate(
          () =>
            (
              window as typeof window & {
                __VERBATIM_ANDROID_BRIDGE_CALLS__?: string[];
              }
            ).__VERBATIM_ANDROID_BRIDGE_CALLS__ ?? [],
        ),
      )
      .toContain("requestSpeechModelDownload");
  });

  test("android home does not require a downloaded desktop model", async ({
    page,
  }) => {
    await installTauriMocks(page, {}, [], "android", {
      availableModels: models.map((model) => ({
        ...model,
        is_downloaded: false,
      })),
      currentModel: "",
      hasAnyModels: false,
    });
    await installAndroidBridgeMock(page, {
      microphone: true,
      overlay: true,
      accessibility: true,
      bubbleRunning: true,
      speechRecognizerAvailable: true,
      onDeviceSpeechRecognizerAvailable: true,
      onDeviceSpeechLanguageAvailable: true,
    });

    await page.goto("/");

    await expect(
      page.getByRole("heading", { name: "Set up mobile dictation" }),
    ).toHaveCount(0);
    await expect(
      page.getByRole("heading", {
        name: "Tap the Verbatim bubble to dictate anywhere",
      }),
    ).toBeVisible();
    await expect(page.getByText("Bubble visibility")).toBeVisible();
  });

  test("android home reads native transcript history from bridge", async ({
    page,
  }) => {
    await installTauriMocks(page, {}, [], "android");
    await installAndroidBridgeMock(
      page,
      {
        microphone: true,
        overlay: true,
        accessibility: true,
        bubbleRunning: true,
        speechRecognizerAvailable: true,
        onDeviceSpeechRecognizerAvailable: true,
        onDeviceSpeechLanguageAvailable: true,
      },
      [
        {
          id: 101,
          timestamp: 1781740000000,
          title: "Android dictation",
          transcription_text: "Native Android raw transcript",
          post_processed_text: "Native Android formatted transcript",
          insertion_status: "inserted",
        },
      ],
    );

    await page.goto("/");

    await expect(
      page.getByRole("heading", {
        name: "Tap the Verbatim bubble to dictate anywhere",
      }),
    ).toBeVisible();
    await expect(page.getByText("Bubble visibility")).toBeVisible();
    await expect(page.getByText("Waiting for keyboard")).toBeVisible();
    await expect(
      page.getByText("Native Android formatted transcript"),
    ).toBeVisible();

    // Regression: native timestamps are ms but formatDateTime expects Unix seconds.
    // The 1781740000000 ms entry must render in 2026, not mis-scale to year 58436.
    await expect(page.locator("body")).toContainText("2026");
    await expect(page.locator("body")).not.toContainText("58436");

    await page.getByRole("button", { name: "History" }).click();
    await expect(
      page.getByText("Native Android formatted transcript"),
    ).toBeVisible();
  });

  test("android Models tab auto-selects a first downloaded ASR pack", async ({
    page,
  }) => {
    await installTauriMocks(page, {}, [], "android", { currentModel: "" });
    await installAndroidBridgeMock(page, {
      microphone: true,
      overlay: true,
      accessibility: true,
      bubbleRunning: true,
      speechRecognizerAvailable: true,
      onDeviceSpeechRecognizerAvailable: true,
      onDeviceSpeechLanguageAvailable: true,
    });

    await page.goto("/");
    await page.getByRole("button", { name: "Models" }).click();

    const packCard = page
      .getByRole("article")
      .filter({ hasText: "English On-device Starter" });
    await expect(
      packCard.getByRole("heading", { name: "English On-device Starter" }),
    ).toBeVisible();
    await expect(packCard.getByText("Available")).toBeVisible();
    await packCard.getByRole("button", { name: "Download" }).click();

    await expect(packCard.getByText("Downloading 42%")).toBeVisible();
    await expect(packCard.getByText("Verifying")).toBeVisible();
    await expect(packCard.getByText("Active")).toBeVisible();
    await expect(packCard.getByRole("button", { name: "Select" })).toHaveCount(
      0,
    );

    await page.getByRole("button", { name: "Home" }).click();
    await expect(page.getByText("English On-device Starter")).toBeVisible();
    await expect(page.getByText("No model selected")).toHaveCount(0);

    await page.getByRole("button", { name: "Models" }).click();

    await packCard.getByRole("button", { name: "Delete" }).click();
    await expect(packCard.getByText("Available")).toBeVisible();

    await expect
      .poll(() =>
        page.evaluate(
          () =>
            (
              window as typeof window & {
                __VERBATIM_TEST_COMMANDS__?: string[];
              }
            ).__VERBATIM_TEST_COMMANDS__ ?? [],
        ),
      )
      .toEqual(
        expect.arrayContaining([
          "asr_list_model_packs",
          "asr_download_model_pack",
          "asr_select_model_pack",
          "asr_delete_model_pack",
        ]),
      );
    await expect
      .poll(() =>
        page.evaluate(
          () =>
            (
              window as typeof window & {
                __VERBATIM_ANDROID_BRIDGE_CALLS__?: string[];
              }
            ).__VERBATIM_ANDROID_BRIDGE_CALLS__ ?? [],
        ),
      )
      .toContain(
        "setEngineModelId:/data/user/0/com.galaxyruler.verbatim/models/android-asr/g3-zipformer-whisper-tiny-en",
      );
  });

  test("android Models tab keeps an existing active ASR pack when downloading another", async ({
    page,
  }) => {
    await installTauriMocks(page, {}, [], "android", {
      currentModel: "",
      androidAsrPacks: [
        {
          id: "g3-zipformer-whisper-tiny-en",
          displayName: "English On-device Starter",
          description: "Streaming Zipformer + Whisper tiny.en + Silero VAD",
          sizeMb: 141,
          installedDir:
            "/data/user/0/com.galaxyruler.verbatim/models/android-asr/g3-zipformer-whisper-tiny-en",
          isInstalled: true,
          isDownloading: false,
          isActive: true,
          isSelectable: true,
          downloadPhase: "ready",
          downloadProgress: 100,
          missingFiles: [],
        },
        {
          id: "g3-alternate-pack",
          displayName: "English Alternate Pack",
          description: "Alternate Android ASR fixture",
          sizeMb: 200,
          installedDir:
            "/data/user/0/com.galaxyruler.verbatim/models/android-asr/g3-alternate-pack",
          isInstalled: false,
          isDownloading: false,
          isActive: false,
          isSelectable: false,
          downloadPhase: "available",
          downloadProgress: 0,
          missingFiles: ["streaming/encoder.onnx"],
        },
      ],
    });
    await installAndroidBridgeMock(page, {
      microphone: true,
      overlay: true,
      accessibility: true,
      bubbleRunning: true,
      speechRecognizerAvailable: true,
      onDeviceSpeechRecognizerAvailable: true,
      onDeviceSpeechLanguageAvailable: true,
    });

    await page.goto("/");
    await page.getByRole("button", { name: "Models" }).click();

    const activePackCard = page
      .getByRole("article")
      .filter({ hasText: "English On-device Starter" });
    const alternatePackCard = page
      .getByRole("article")
      .filter({ hasText: "English Alternate Pack" });

    await expect(activePackCard.getByText("Active")).toBeVisible();
    await alternatePackCard.getByRole("button", { name: "Download" }).click();
    await expect(alternatePackCard.getByText("Ready")).toBeVisible();
    await expect(alternatePackCard.getByText("Active")).toHaveCount(0);
    await expect(activePackCard.getByText("Active")).toBeVisible();

    const bridgeCalls = await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_ANDROID_BRIDGE_CALLS__?: string[];
      };
      return win.__VERBATIM_ANDROID_BRIDGE_CALLS__ ?? [];
    });
    expect(bridgeCalls).not.toContain(
      "setEngineModelId:/data/user/0/com.galaxyruler.verbatim/models/android-asr/g3-alternate-pack",
    );
  });

  test("android Models tab auto-selects a first downloaded cleanup pack", async ({
    page,
  }) => {
    await installTauriMocks(page, {}, [], "android");
    await installAndroidBridgeMock(page, {
      microphone: true,
      overlay: true,
      accessibility: true,
      bubbleRunning: true,
      speechRecognizerAvailable: true,
      onDeviceSpeechRecognizerAvailable: true,
      onDeviceSpeechLanguageAvailable: true,
      llmPostProcessingSupported: true,
    });

    await page.goto("/");
    await page.getByRole("button", { name: "Models" }).click();

    const cleanupCard = page
      .getByRole("article")
      .filter({ hasText: "Qwen2.5 cleanup 0.5B" });
    await expect(cleanupCard.getByText("Available")).toBeVisible();
    await cleanupCard.getByRole("button", { name: "Download" }).click();

    await expect(cleanupCard.getByText("Downloading 42%")).toBeVisible();
    await expect(cleanupCard.getByText("Verifying")).toBeVisible();
    await expect(cleanupCard.getByText("Active")).toBeVisible();
    await expect(
      cleanupCard.getByRole("button", { name: "Select" }),
    ).toHaveCount(0);

    await expect
      .poll(() =>
        page.evaluate(
          () =>
            (
              window as typeof window & {
                __VERBATIM_ANDROID_BRIDGE_CALLS__?: string[];
              }
            ).__VERBATIM_ANDROID_BRIDGE_CALLS__ ?? [],
        ),
      )
      .toContain(
        "setLlmModelId:/data/user/0/com.galaxyruler.verbatim/models/android-llm-postproc/g4-qwen2_5-0_5b-litert-q8",
      );
  });

  test("android shell syncs formatter rules to native bridge", async ({
    page,
  }) => {
    await installTauriMocks(
      page,
      {
        dictionary_entries: [
          {
            id: "dict_android_1",
            phrase: "Kulaib",
            replacement_of: "club",
            source: "manual",
            priority: "starred",
            created_at_ms: 1,
            updated_at_ms: 2,
          },
        ],
        snippets: [
          {
            id: "snippet_android_1",
            trigger: "/sig",
            content: "Sent from Verbatim Android",
            created_at_ms: 1,
            updated_at_ms: 2,
          },
        ],
      },
      [],
      "android",
    );
    await installAndroidBridgeMock(page, {
      microphone: true,
      overlay: true,
      accessibility: true,
      bubbleRunning: true,
      speechRecognizerAvailable: true,
      onDeviceSpeechRecognizerAvailable: true,
      onDeviceSpeechLanguageAvailable: true,
    });

    await page.goto("/");

    await expect
      .poll(() =>
        page.evaluate(() => {
          const win = window as typeof window & {
            __VERBATIM_ANDROID_FORMATTER_SNAPSHOTS__?: string[];
          };
          const lastSnapshot =
            win.__VERBATIM_ANDROID_FORMATTER_SNAPSHOTS__?.at(-1);
          return lastSnapshot ? JSON.parse(lastSnapshot) : null;
        }),
      )
      .toEqual(
        expect.objectContaining({
          dictionary_entries: expect.arrayContaining([
            expect.objectContaining({
              phrase: "Kulaib",
              replacement_of: "club",
              priority: "starred",
            }),
          ]),
          snippets: expect.arrayContaining([
            expect.objectContaining({
              trigger: "/sig",
              content: "Sent from Verbatim Android",
            }),
          ]),
        }),
      );
  });

  test("android settings opens dictionary and snippets library screens", async ({
    page,
  }) => {
    await installTauriMocks(
      page,
      {
        dictionary_entries: [
          {
            id: "dict_android_1",
            phrase: "Kulaib",
            replacement_of: "club",
            source: "manual",
            priority: "starred",
            created_at_ms: 1,
            updated_at_ms: 2,
          },
        ],
        snippets: [
          {
            id: "snippet_android_1",
            trigger: "/sig",
            content: "Sent from Verbatim Android",
            created_at_ms: 1,
            updated_at_ms: 2,
          },
        ],
      },
      [],
      "android",
    );
    await installAndroidBridgeMock(page, {
      microphone: true,
      overlay: true,
      accessibility: true,
      bubbleRunning: true,
      speechRecognizerAvailable: true,
      onDeviceSpeechRecognizerAvailable: true,
      onDeviceSpeechLanguageAvailable: true,
    });

    await page.goto("/");

    await expect(page.getByRole("button", { name: "Dictionary" })).toHaveCount(
      0,
    );
    await page.getByRole("button", { name: "Settings" }).click();
    await page.getByRole("button", { name: "Advanced features" }).click();
    await page.getByRole("button", { name: "Dictionary" }).click();
    await expect(page.getByText("Kulaib")).toBeVisible();
    await expect(page.getByText("Corrects: club")).toBeVisible();

    await page.getByLabel("Word or phrase").fill("Verbatim Android");
    await page.getByLabel("Correct when Verbatim writes").fill("Handy Android");
    await page.getByRole("button", { name: "Add entry" }).click();
    await expect(page.getByText("Verbatim Android")).toBeVisible();

    await page.getByRole("button", { name: "Edit Kulaib" }).click();
    await page.getByLabel("Word or phrase").fill("Kulaib updated");
    await page.getByRole("button", { name: "Save entry" }).click();
    await expect(page.getByText("Kulaib updated")).toBeVisible();

    await page.getByRole("button", { name: "Delete Kulaib updated" }).click();
    await expect(page.getByText("Kulaib updated")).toHaveCount(0);

    await page.getByRole("tab", { name: "Snippets" }).click();
    await expect(page.getByText("/sig")).toBeVisible();
    await expect(page.getByText("Sent from Verbatim Android")).toBeVisible();

    await page.getByLabel("Trigger phrase").fill("/addr");
    await page.getByLabel("Snippet content").fill("Android address block");
    await page.getByRole("button", { name: "Add snippet" }).click();
    await expect(page.getByText("/addr")).toBeVisible();

    await page.getByRole("button", { name: "Edit /addr" }).click();
    await page.getByLabel("Snippet content").fill("Updated Android address");
    await page.getByRole("button", { name: "Save snippet" }).click();
    await expect(page.getByText("Updated Android address")).toBeVisible();

    await page.getByRole("button", { name: "Delete /addr" }).click();
    await expect(page.getByText("/addr")).toHaveCount(0);

    await page.getByLabel("Cancel").click();
    await expect(page.getByRole("button", { name: "Settings" })).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Advanced features" }),
    ).toBeVisible();
    await expect(page.getByRole("button", { name: "Dictionary" })).toHaveCount(
      0,
    );
  });

  test("android settings opens post-processing screen", async ({ page }) => {
    await installTauriMocks(
      page,
      {
        post_process_enabled: true,
        post_process_provider_id: "openai",
        post_process_providers: [
          {
            id: "openai",
            label: "OpenAI",
            base_url: "https://api.openai.com/v1",
            allow_base_url_edit: false,
            models_endpoint: null,
            supports_structured_output: true,
          },
          {
            id: "anthropic",
            label: "Claude",
            base_url: "https://api.anthropic.com/v1",
            allow_base_url_edit: false,
            models_endpoint: null,
            supports_structured_output: true,
          },
          {
            id: "custom",
            label: "Custom",
            base_url: "http://127.0.0.1:11434/v1",
            allow_base_url_edit: true,
            models_endpoint: null,
            supports_structured_output: false,
          },
          {
            id: "apple_intelligence",
            label: "Apple Intelligence",
            base_url: "apple-intelligence://local",
            allow_base_url_edit: false,
            models_endpoint: null,
            supports_structured_output: true,
          },
        ],
        post_process_api_keys: {
          openai: "test-openai-key",
          anthropic: "test-claude-key",
        },
        post_process_models: {
          openai: "gpt-4o-mini",
          anthropic: "claude-3-5-haiku-latest",
        },
        post_process_prompts: [
          {
            id: "prompt_clean",
            name: "Clean up",
            prompt: "Clean up {transcription}",
          },
        ],
        post_process_selected_prompt_id: "prompt_clean",
      },
      [],
      "android",
    );
    await installAndroidBridgeMock(page, {
      microphone: true,
      overlay: true,
      accessibility: true,
      bubbleRunning: true,
      speechRecognizerAvailable: true,
      onDeviceSpeechRecognizerAvailable: true,
      onDeviceSpeechLanguageAvailable: true,
    });

    await page.goto("/");

    await page.getByRole("button", { name: "Settings" }).click();
    await page.getByRole("button", { name: "Advanced features" }).click();
    await page.getByRole("button", { name: "Post-processing" }).click();

    await expect(
      page.getByRole("heading", { name: "Post-processing", exact: true }),
    ).toBeVisible();
    await expect(page.getByRole("button", { name: "Home" })).toHaveCount(0);
    await expect(page.getByText("Apple Intelligence")).toHaveCount(0);
    await expect(page.getByRole("button", { name: /OpenAI/ })).toBeVisible();
    await expect(page.getByLabel("API key")).toHaveValue("test-openai-key");
    const modelField = page.getByRole("combobox", {
      name: "Model",
      exact: true,
    });
    await expect(modelField).toHaveValue("gpt-4o-mini");
    await expect(page.getByLabel("Selected prompt")).toHaveValue(
      "prompt_clean",
    );

    await page.getByRole("button", { name: /Claude/ }).click();
    await expect(modelField).toHaveValue("claude-3-5-haiku-latest");
    await page.getByRole("button", { name: "Refresh models" }).click();
    await modelField.fill("claude-3-5-sonnet-latest");
    await page.getByLabel("API key").click();
    await expect(modelField).toHaveValue("claude-3-5-sonnet-latest");

    await page.getByRole("button", { name: "Create new prompt" }).click();
    await expect(
      page.getByRole("heading", { name: "Create new prompt" }),
    ).toBeVisible();
    await page.getByLabel("Prompt label").fill("Android polish");
    await page
      .getByLabel("Prompt instructions")
      .fill("Polish {transcription} for mobile.");
    await page.getByRole("button", { name: "Save" }).click();
    await expect(page.getByLabel("Selected prompt")).toHaveValue(
      "prompt_test_1",
    );

    await expect
      .poll(async () =>
        page.evaluate(() => {
          const win = window as typeof window & {
            __VERBATIM_TEST_COMMANDS__: string[];
            __VERBATIM_TEST_INVOKES__: Array<{
              cmd: string;
              args?: Record<string, unknown>;
            }>;
          };
          return {
            commands: win.__VERBATIM_TEST_COMMANDS__,
            invokes: win.__VERBATIM_TEST_INVOKES__,
          };
        }),
      )
      .toEqual(
        expect.objectContaining({
          commands: expect.arrayContaining([
            "set_post_process_provider",
            "fetch_post_process_models",
            "change_post_process_model_setting",
            "add_post_process_prompt",
            "set_post_process_selected_prompt",
          ]),
          invokes: expect.arrayContaining([
            expect.objectContaining({
              cmd: "change_post_process_model_setting",
              args: {
                providerId: "anthropic",
                model: "claude-3-5-sonnet-latest",
              },
            }),
          ]),
        }),
      );

    await page.getByLabel("Cancel").click();
    await expect(
      page.getByRole("button", { name: "Advanced features" }),
    ).toBeVisible();
    await page.getByRole("button", { name: "Advanced features" }).click();
    await expect(
      page.getByRole("button", { name: "Post-processing" }),
    ).toBeVisible();
  });

  test("android settings expose mobile configuration sheets", async ({
    page,
  }) => {
    await installTauriMocks(
      page,
      {
        audio_feedback: true,
        audio_feedback_volume: 0.25,
        selected_microphone: "Default",
        selected_output_device: "Default",
        history_limit: 100,
        recording_retention_period: "never",
        sound_theme: "marimba",
        app_language: "en",
      },
      [],
      "android",
    );
    await installAndroidBridgeMock(
      page,
      {
        microphone: true,
        overlay: true,
        accessibility: true,
        bubbleRunning: true,
        speechRecognizerAvailable: true,
        onDeviceSpeechRecognizerAvailable: true,
        onDeviceSpeechLanguageAvailable: true,
      },
      [],
      "top-right",
    );

    await page.goto("/");
    await page.getByRole("button", { name: "Settings" }).click();

    const volumeSlider = page.getByLabel("Volume");
    await volumeSlider.focus();
    await volumeSlider.press("End");
    await expect(page.getByText("100%")).toBeVisible();

    await page.getByRole("button", { name: /Bubble position/ }).click();
    await expect(
      page.getByRole("dialog", { name: "Bubble position" }),
    ).toBeVisible();
    await page.getByRole("button", { name: /Bottom right/ }).click();
    await expect(page.getByText("Bottom right")).toBeVisible();

    await page.getByRole("button", { name: /App display language/ }).click();
    const languageDialog = page.getByRole("dialog", {
      name: "App display language",
    });
    await expect(languageDialog).toBeVisible();
    await expect(
      languageDialog.getByRole("button", { name: /English/ }),
    ).toBeVisible();
    await page.getByLabel("Cancel").click();

    await page.getByRole("button", { name: "Advanced features" }).click();
    await page.getByRole("button", { name: /History limit/ }).click();
    await page.getByRole("spinbutton", { name: "History limit" }).fill("250");
    await page.getByRole("button", { name: "Save" }).click();
    await expect(page.getByText("250 entries")).toBeVisible();

    await page.getByRole("button", { name: /Auto-delete recordings/ }).click();
    await page.getByRole("button", { name: "After 3 days" }).click();
    await expect(page.getByText("After 3 days")).toBeVisible();

    await page.getByRole("button", { name: /Sound theme/ }).click();
    await page.getByRole("button", { name: "Pop" }).click();
    await expect(page.getByText("Pop")).toBeVisible();

    await expect
      .poll(async () =>
        page.evaluate(() => {
          const win = window as typeof window & {
            __VERBATIM_ANDROID_BRIDGE_CALLS__: string[];
            __VERBATIM_TEST_COMMANDS__: string[];
            __VERBATIM_TEST_INVOKES__: Array<{
              cmd: string;
              args?: Record<string, unknown>;
            }>;
          };
          return {
            bridge: win.__VERBATIM_ANDROID_BRIDGE_CALLS__,
            commands: win.__VERBATIM_TEST_COMMANDS__,
            invokes: win.__VERBATIM_TEST_INVOKES__,
          };
        }),
      )
      .toEqual(
        expect.objectContaining({
          bridge: expect.arrayContaining(["setBubbleCorner:bottom-right"]),
          commands: expect.arrayContaining([
            "change_audio_feedback_volume_setting",
            "update_history_limit",
            "update_recording_retention_period",
            "change_sound_theme_setting",
          ]),
          invokes: expect.arrayContaining([
            expect.objectContaining({
              cmd: "change_audio_feedback_volume_setting",
              args: { volume: 1 },
            }),
            expect.objectContaining({
              cmd: "update_history_limit",
              args: { limit: 250 },
            }),
            expect.objectContaining({
              cmd: "update_recording_retention_period",
              args: { period: "days3" },
            }),
            expect.objectContaining({
              cmd: "change_sound_theme_setting",
              args: { theme: "pop" },
            }),
          ]),
        }),
      );
  });

  test("android settings toggle on-device engine while OS recognizer remains fallback", async ({
    page,
  }) => {
    await installTauriMocks(
      page,
      {
        audio_feedback: true,
        app_language: "en",
      },
      [],
      "android",
    );
    await installAndroidBridgeMock(page, {
      microphone: true,
      overlay: true,
      accessibility: true,
      bubbleRunning: true,
      speechRecognizerAvailable: true,
      onDeviceSpeechRecognizerAvailable: true,
      onDeviceSpeechLanguageAvailable: true,
    });

    await page.goto("/");
    await page.getByRole("button", { name: "Settings" }).click();

    const engineToggle = page.getByRole("button", {
      name: "On-device engine (beta)",
    });
    await expect(engineToggle).toHaveAttribute("aria-pressed", "false");
    await expect(page.getByText("OS speech recognizer fallback")).toBeVisible();

    await engineToggle.click();

    await expect(engineToggle).toHaveAttribute("aria-pressed", "true");
    await expect(page.getByText("Verbatim engine")).toBeVisible();
    await expect
      .poll(async () =>
        page.evaluate(() => {
          const win = window as typeof window & {
            __VERBATIM_ANDROID_BRIDGE_CALLS__: string[];
          };
          return win.__VERBATIM_ANDROID_BRIDGE_CALLS__;
        }),
      )
      .toContain("setEngineDictationEnabled:true");
  });

  test("android settings keeps LLM cleanup off and disabled when unsupported", async ({
    page,
  }) => {
    await installTauriMocks(
      page,
      {
        audio_feedback: true,
        app_language: "en",
      },
      [],
      "android",
    );
    await installAndroidBridgeMock(page, {
      microphone: true,
      overlay: true,
      accessibility: true,
      bubbleRunning: true,
      speechRecognizerAvailable: true,
      onDeviceSpeechRecognizerAvailable: true,
      onDeviceSpeechLanguageAvailable: true,
      llmPostProcessingSupported: false,
      llmPostProcessingReason: "requires8GbRam",
      llmTotalRamMb: 6144,
      llmAvailableRamMb: 2048,
      llmMinRamMb: 8192,
      llmHardware: "qcom",
      llmSocModel: "SM8550",
    });

    await page.goto("/");
    await page.getByRole("button", { name: "Settings" }).click();

    const cleanupToggle = page.getByRole("button", {
      name: "On-device cleanup (beta)",
    });
    await expect(cleanupToggle).toHaveAttribute("aria-pressed", "false");
    await expect(cleanupToggle).toBeDisabled();
    await expect(page.getByText("Requires at least 8 GB RAM.")).toBeVisible();

    await cleanupToggle.click({ force: true });
    await expect
      .poll(async () =>
        page.evaluate(() => {
          const win = window as typeof window & {
            __VERBATIM_ANDROID_BRIDGE_CALLS__: string[];
          };
          return win.__VERBATIM_ANDROID_BRIDGE_CALLS__;
        }),
      )
      .not.toContain("setLlmPostProcessingEnabled:true");
  });

  test("android settings exposes about source and license details", async ({
    page,
  }) => {
    await installTauriMocks(page, {}, [], "android");
    await installAndroidBridgeMock(page, {
      microphone: true,
      overlay: true,
      accessibility: true,
      bubbleRunning: true,
      speechRecognizerAvailable: true,
      onDeviceSpeechRecognizerAvailable: true,
      onDeviceSpeechLanguageAvailable: true,
    });

    await page.goto("/");
    await page.getByRole("button", { name: "Settings" }).click();

    await page.getByRole("button", { name: /^About/ }).click();
    await expect(
      page.getByRole("heading", { name: "About" }).first(),
    ).toBeVisible();
    await expect(page.getByText("MIT License Notice")).toBeVisible();
    await expect(
      page.getByText(/Portions of this app are derived from Handy/),
    ).toBeVisible();
    await expect(
      page.getByRole("heading", { name: "Whisper.cpp" }),
    ).toBeVisible();

    await page.getByRole("button", { name: /View on GitHub/ }).click();
    await page.getByRole("button", { name: /View Handy on GitHub/ }).click();

    await expect
      .poll(() =>
        page.evaluate(() => {
          const win = window as typeof window & {
            __VERBATIM_ANDROID_BRIDGE_CALLS__: string[];
          };
          return win.__VERBATIM_ANDROID_BRIDGE_CALLS__;
        }),
      )
      .toEqual(
        expect.arrayContaining([
          "openExternalUrl:https://github.com/GalaxyRuler/Verbatim",
          "openExternalUrl:https://github.com/cjpais/Handy",
        ]),
      );
  });

  test("adaptive profiles can be enabled from experimental settings", async ({
    page,
  }) => {
    const pageErrors: string[] = [];
    page.on("pageerror", (error) => pageErrors.push(error.message));
    await installTauriMocks(page);
    await page.goto("/");

    await expect
      .poll(() => pageErrors, {
        message: "page should not throw before rendering settings",
      })
      .toEqual([]);
    await expect(page.getByTitle("General")).toBeVisible();
    await page.getByText("Advanced").click();
    await expect(page.getByText("Adaptive profiles")).toHaveCount(0);

    await settingRow(page, "Experimental features")
      .getByRole("checkbox")
      .check({ force: true });

    const adaptiveRow = settingRow(page, "Adaptive profiles");
    await expect(adaptiveRow).toBeVisible();
    await expect(page.getByText("Default profile")).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Reprocess last" }),
    ).toBeVisible();

    const adaptiveToggle = adaptiveRow.getByRole("checkbox");
    await expect(adaptiveToggle).not.toBeChecked();
    await adaptiveToggle.check({ force: true });
    await expect(adaptiveToggle).toBeChecked();
  });

  test("adaptive profile settings expose privacy-gated context controls", async ({
    page,
  }) => {
    await installTauriMocks(page);
    await page.goto("/");
    await expect(page.getByTitle("General")).toBeVisible();
    await page.getByText("Advanced").click();

    await settingRow(page, "Experimental features")
      .getByRole("checkbox")
      .check({ force: true });

    await expect(page.getByText("Context awareness")).toBeVisible();
    await expect(page.getByText("Nearby text")).toBeVisible();
    await expect(
      settingRow(page, "Nearby text").getByRole("checkbox"),
    ).toBeDisabled();

    await settingRow(page, "Context awareness")
      .getByRole("checkbox")
      .check({ force: true });
    await expect(
      settingRow(page, "Nearby text").getByRole("checkbox"),
    ).toBeEnabled();
    await settingRow(page, "Nearby text")
      .getByRole("checkbox")
      .check({ force: true });
    await expect(
      settingRow(page, "Nearby text").getByRole("checkbox"),
    ).toBeChecked();

    await settingRow(page, "Context awareness")
      .getByRole("checkbox")
      .uncheck({ force: true });
    await expect(
      settingRow(page, "Nearby text").getByRole("checkbox"),
    ).toBeDisabled();
    await expect(
      settingRow(page, "Nearby text").getByRole("checkbox"),
    ).not.toBeChecked();

    const invokes = await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_INVOKES__: Array<{
          cmd: string;
          args?: Record<string, unknown>;
        }>;
      };
      return win.__VERBATIM_TEST_INVOKES__;
    });

    expect(invokes).toContainEqual({
      cmd: "change_context_awareness_enabled_setting",
      args: { enabled: true },
    });
    expect(invokes).toContainEqual({
      cmd: "change_context_nearby_text_enabled_setting",
      args: { enabled: true },
    });
    expect(invokes).toContainEqual({
      cmd: "change_context_awareness_enabled_setting",
      args: { enabled: false },
    });
  });

  test("general settings show unbound selected-text transform shortcuts", async ({
    page,
  }) => {
    await installTauriMocks(page, {
      bindings: defaultBindings,
    });
    await page.goto("/");

    await expect(
      page.getByRole("heading", { name: "Transform selected text" }),
    ).toBeVisible();
    await expect(
      page.getByText(
        "Assign optional shortcuts that transform selected text in the current app using your configured local or remote post-processing provider.",
      ),
    ).toBeVisible();

    for (const name of [
      "Polish selected text",
      "Make selected text concise",
      "Turn selected text into a list",
      "Translate selected text",
      "Prompt-engineer selected text",
    ]) {
      await expect(page.getByText(name)).toBeVisible();
    }

    await expect(page.getByText("Unassigned")).toHaveCount(5);
  });

  test("general settings show selected-text transform shortcuts on Linux", async ({
    page,
  }) => {
    await installTauriMocks(
      page,
      {
        bindings: defaultBindings,
      },
      [],
      "linux",
    );
    await page.goto("/");

    await expect(
      page.getByRole("heading", { name: "Transform selected text" }),
    ).toBeVisible();
    await expect(page.getByText("Polish selected text")).toBeVisible();
  });

  test("formatting level can be changed from advanced transcription settings", async ({
    page,
  }) => {
    await installTauriMocks(page);
    await page.goto("/");

    await expect(page.getByTitle("General")).toBeVisible();
    await page.getByText("Advanced").click();

    const formattingRow = settingRow(page, "Smart formatting");
    await expect(formattingRow).toBeVisible();
    await formattingRow.getByRole("button", { name: "Light" }).click();
    await page.getByRole("button", { name: "Medium" }).click();

    const invokes = await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_INVOKES__: Array<{
          cmd: string;
          args?: Record<string, unknown>;
        }>;
      };
      return win.__VERBATIM_TEST_INVOKES__;
    });
    expect(invokes).toContainEqual(
      expect.objectContaining({
        cmd: "change_formatting_level_setting",
        args: expect.objectContaining({ level: "medium" }),
      }),
    );
  });

  test("formatting level explains each cleanup strength", async ({ page }) => {
    await installTauriMocks(page);
    await page.goto("/");

    await expect(page.getByTitle("General")).toBeVisible();
    await page.getByText("Advanced").click();

    const formattingRow = settingRow(page, "Smart formatting");
    await expect(formattingRow).toBeVisible();
    await expect(
      formattingRow.getByText("Safe spacing and spoken corrections."),
    ).toBeVisible();

    await formattingRow.getByRole("button", { name: "Light" }).click();
    await expect(
      page.getByText("Adds filler removal and spoken punctuation."),
    ).toBeVisible();

    await page.getByRole("button", { name: "Medium" }).click();
    await expect(
      formattingRow.getByText("Adds filler removal and spoken punctuation."),
    ).toBeVisible();
  });

  test("dictionary section is visible from the sidebar", async ({ page }) => {
    await installTauriMocks(page);
    await page.goto("/");

    await expect(page.getByTitle("General")).toBeVisible();
    await page.getByText("Dictionary").click();
    await expect(
      page.getByRole("heading", { name: "Dictionary" }),
    ).toBeVisible();
  });

  test("snippets section is visible from the sidebar", async ({ page }) => {
    await installTauriMocks(page);
    await page.goto("/");

    await page.getByText("Snippets").click();
    await expect(page.getByRole("heading", { name: "Snippets" })).toBeVisible();
  });

  test("local post-processing model panel is hidden when post-processing is disabled", async ({
    page,
  }) => {
    await installTauriMocks(page);
    await page.goto("/");

    await expect(page.getByTitle("General")).toBeVisible();
    await page.getByTitle("Post-processing").click();

    await expect(
      page.getByText("Enable post-processing", { exact: true }),
    ).toBeVisible();
    await expect(page.getByText("Hotkey", { exact: true })).toHaveCount(0);
    await expect(page.getByText("Managed local model")).toHaveCount(0);
  });

  test("local post-processing model can be downloaded selected and enabled", async ({
    page,
  }) => {
    await installTauriMocks(page, { post_process_enabled: true });
    await page.goto("/");

    await expect(page.getByTitle("General")).toBeVisible();
    await page.getByTitle("Post-processing").click();
    await page.getByRole("button", { name: "Download" }).click();
    await page.getByRole("button", { name: "Select", exact: true }).click();
    await page.getByRole("button", { name: "Enable" }).click();

    const result = await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_COMMANDS__: string[];
        __VERBATIM_TEST_INVOKES__: Array<{
          cmd: string;
          args?: Record<string, unknown>;
        }>;
      };
      return {
        commands: win.__VERBATIM_TEST_COMMANDS__,
        invokes: win.__VERBATIM_TEST_INVOKES__,
      };
    });

    expect(result.commands).toContain("download_local_llm_model");
    expect(result.commands).toContain("select_local_llm_model");
    expect(result.commands).toContain("set_local_llm_enabled");
    expect(result.invokes).toContainEqual(
      expect.objectContaining({
        cmd: "set_local_llm_enabled",
        args: { enabled: true },
      }),
    );
    await expect(page.getByRole("button", { name: "Disable" })).toBeVisible();
  });

  test("post-processing shows an explicit API or local engine choice", async ({
    page,
  }) => {
    await installTauriMocks(page, { post_process_enabled: true });
    await page.goto("/");

    await expect(page.getByTitle("General")).toBeVisible();
    await page.getByTitle("Post-processing").click();

    await expect(page.getByText("Processing engine")).toBeVisible();
    await expect(
      page.getByRole("button", { name: "API provider" }),
    ).toHaveAttribute("aria-pressed", "true");
    await expect(
      page.getByRole("button", { name: "Local model" }),
    ).toHaveAttribute("aria-pressed", "false");
    await expect(page.getByRole("heading", { name: "Provider" })).toBeVisible();

    await page.getByRole("button", { name: "Download" }).click();
    await page.getByRole("button", { name: "Select", exact: true }).click();
    await page.getByRole("button", { name: "Local model" }).click();

    await expect(
      page.getByRole("button", { name: "Local model" }),
    ).toHaveAttribute("aria-pressed", "true");
    await expect(page.getByRole("heading", { name: "Provider" })).toHaveCount(
      0,
    );
  });

  test("snippet entry can be added, searched, edited, and deleted", async ({
    page,
  }) => {
    await installTauriMocks(page);
    await page.goto("/");

    await page.getByText("Snippets").click();
    await page.getByRole("button", { name: "Add snippet" }).click();
    await page.getByLabel("Trigger phrase").fill("email signature");
    await page.getByLabel("Snippet content").fill("Regards,\nAbdullah");
    await page.getByRole("button", { name: "Save snippet" }).click();

    await expect(page.getByText("email signature")).toBeVisible();
    await expect(page.getByText("Regards, Abdullah")).toBeVisible();
    await page.getByLabel("Search snippets").fill("signature");
    await expect(page.getByText("email signature")).toBeVisible();

    await page.getByRole("button", { name: "Edit email signature" }).click();
    await page.getByLabel("Trigger phrase").fill("formal email signature");
    await page.getByLabel("Snippet content").fill("Sincerely,\nAbdullah");
    await page.getByRole("button", { name: "Save snippet" }).click();

    await expect(page.getByText("formal email signature")).toBeVisible();
    await expect(
      page.getByText("email signature", { exact: true }),
    ).toHaveCount(0);

    await page
      .getByRole("button", { name: "Delete formal email signature" })
      .click();
    await expect(page.getByText("formal email signature")).toHaveCount(0);
  });

  test("manual dictionary entry can be added and searched", async ({
    page,
  }) => {
    await installTauriMocks(page);
    await page.goto("/");

    await page.getByText("Dictionary").click();
    await page.getByRole("button", { name: "Add entry" }).click();
    await page.getByLabel("Word or phrase").fill("Abdullah al Kulaib");
    await page.getByRole("button", { name: "Save entry" }).click();

    await expect(page.getByText("Abdullah al Kulaib")).toBeVisible();
    await page.getByLabel("Search dictionary").fill("kulaib");
    await expect(page.getByText("Abdullah al Kulaib")).toBeVisible();
    await expect(page.getByText("No entries match your search.")).toHaveCount(
      0,
    );
  });

  test("manual entry supports correction mapping and editing", async ({
    page,
  }) => {
    await installTauriMocks(page);
    await page.goto("/");

    await page.getByText("Dictionary").click();
    await page.getByRole("button", { name: "Add entry" }).click();
    await page.getByLabel("Word or phrase").fill("Robyn");
    await page.getByLabel("Correct when Verbatim writes").fill("robin");
    await page.getByRole("button", { name: "Save entry" }).click();

    await expect(page.getByText("Robyn")).toBeVisible();
    await expect(page.getByText("Corrects: robin")).toBeVisible();

    await page.getByRole("button", { name: "Edit Robyn" }).click();
    await page.getByLabel("Word or phrase").fill("Robyn Smith");
    await page.getByRole("button", { name: "Save entry" }).click();

    await expect(page.getByText("Robyn Smith")).toBeVisible();
    await expect(page.getByText("Robyn", { exact: true })).toHaveCount(0);
  });

  test("dictionary entry can be starred and deleted", async ({ page }) => {
    await installTauriMocks(page);
    await page.goto("/");

    await page.getByText("Dictionary").click();
    await page.getByRole("button", { name: "Add entry" }).click();
    await page.getByLabel("Word or phrase").fill("ChargeBee");
    await page.getByRole("button", { name: "Save entry" }).click();

    await page.getByRole("button", { name: "Star ChargeBee" }).click();
    await expect(
      page.getByRole("button", { name: "Unstar ChargeBee" }),
    ).toBeVisible();

    await page.getByRole("button", { name: "Delete ChargeBee" }).click();
    await expect(page.getByText("ChargeBee")).toHaveCount(0);
  });

  test("auto-learn event shows recently learned entry with undo", async ({
    page,
  }) => {
    await installTauriMocks(page);
    await page.goto("/");

    await page.getByText("Dictionary").click();
    await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_LEARN_ENTRIES__: (
          entries: Array<Record<string, unknown>>,
        ) => void;
      };
      win.__VERBATIM_TEST_LEARN_ENTRIES__([
        {
          id: "dict_test_kulaib",
          phrase: "Abdullah al Kulaib",
          replacement_of: "abdullah al kulaib",
          source: "auto_learned",
          priority: "normal",
          created_at_ms: 1,
          updated_at_ms: 1,
        },
      ]);
    });

    const learnedStatus = page.getByTestId("dictionary-recently-learned");
    await expect(learnedStatus).toBeVisible();
    await expect(learnedStatus).toContainText("Dictionary updated");
    await expect(learnedStatus).toContainText(
      "Now correcting: Abdullah al Kulaib",
    );

    await learnedStatus.getByRole("button", { name: "Undo" }).click();
    await expect(page.getByTestId("dictionary-entries-list")).not.toContainText(
      "Abdullah al Kulaib",
    );
  });

  test("pending review queue lists a learn candidate and approves it", async ({
    page,
  }) => {
    await installTauriMocks(page, {}, [], "windows", {
      learnCandidates: [
        {
          replacement_of: "robin",
          phrase: "Robyn",
          occurrences: 1,
          last_evidence_session: null,
          created_at_ms: 1,
          updated_at_ms: 1,
        },
      ],
    });
    await page.goto("/");

    await page.getByText("Dictionary").click();

    const pendingSection = page.getByTestId("dictionary-pending-review");
    await expect(pendingSection).toBeVisible();
    await expect(pendingSection).toContainText("Robyn");
    await expect(pendingSection).toContainText('from "robin"');

    await pendingSection.getByRole("button", { name: "Approve" }).click();

    const result = await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_INVOKES__: Array<{
          cmd: string;
          args?: Record<string, unknown>;
        }>;
      };
      return win.__VERBATIM_TEST_INVOKES__;
    });
    expect(
      result.some((invoke) => invoke.cmd === "approve_learn_candidate"),
    ).toBe(true);

    await expect(pendingSection).toHaveCount(0);
    await expect(page.getByTestId("dictionary-entries-list")).toContainText(
      "Robyn",
    );
  });

  test("pending review section is absent with no candidates", async ({
    page,
  }) => {
    await installTauriMocks(page, {}, [], "windows", {
      learnCandidates: [],
    });
    await page.goto("/");

    await page.getByText("Dictionary").click();

    await expect(page.getByTestId("dictionary-pending-review")).toHaveCount(0);
  });

  test("learning diagnostics panel shows outcome counts", async ({ page }) => {
    await installTauriMocks(page, {}, [], "windows", {
      dictionaryDiagnostics: {
        learned: 3,
        promoted: 1,
        reinforced: 2,
        skip_secure_field: 1,
        since_ms: 1700000000000,
      },
    });
    await page.goto("/");

    await page.getByText("Dictionary").click();

    const diagnostics = page.getByTestId("dictionary-diagnostics");
    await expect(diagnostics).toBeVisible();
    await diagnostics.locator("summary").click();
    await expect(diagnostics).toContainText("3");
    await expect(diagnostics).toContainText("1");
    await expect(diagnostics).toContainText("2");
    await expect(diagnostics).toContainText("Password fields");
  });

  test("advanced settings no longer owns dictionary management", async ({
    page,
  }) => {
    await installTauriMocks(page);
    await page.goto("/");

    await page.getByText("Advanced").click();
    await expect(page.getByText("Custom words")).toHaveCount(0);
    await expect(page.getByText("Dictionary")).toBeVisible();
  });

  test("transform history entries omit recording-only controls", async ({
    page,
  }) => {
    await installTauriMocks(page, {}, [
      {
        id: 42,
        file_name: "transform-42.txt",
        timestamp: Date.now(),
        saved: false,
        title: "Polished selected text",
        transcription_text: "Raw selected text",
        post_processed_text: "Polished transform result",
        post_process_prompt: null,
        post_process_requested: false,
        adaptive_profile_id: null,
        adaptive_profile_name: null,
        adaptive_routing_json: null,
        adaptive_context_json: null,
        adaptive_language_json: null,
        adaptive_insertion_json: null,
        adaptive_parent_entry_id: null,
        transform_action: "polish",
        transform_original_text: "Raw selected text",
        transform_result_text: "Polished transform result",
        transform_target_language: null,
        transform_provider_id: "local-llm",
        transform_model: "qwen2.5-0.5b",
        transform_recovery_status: "replaced",
      },
    ]);
    await page.goto("/");

    await page.getByText("History & Privacy").click();

    await expect(page.getByText("Polished transform result")).toBeVisible();
    await expect(page.getByText("Raw selected text")).toHaveCount(0);
    await expect(page.getByTitle("Copy transcript to clipboard")).toBeVisible();
    await expect(page.getByTitle("Delete entry")).toBeVisible();

    // Remaining actions live behind the overflow menu.
    await page.getByRole("button", { name: "More actions" }).click();
    await expect(
      page.getByRole("button", { name: "Save transcript" }),
    ).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Learn dictionary correction" }),
    ).toHaveCount(0);
    await expect(
      page.getByRole("button", { name: "Re-transcribe" }),
    ).toHaveCount(0);
    await page.keyboard.press("Escape");
    await expect(
      page.getByRole("button", { name: "Save transcript" }),
    ).toHaveCount(0);
    await expect(page.getByRole("button", { name: "Play" })).toHaveCount(0);

    const commandsInvoked = await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_COMMANDS__: string[];
      };
      return win.__VERBATIM_TEST_COMMANDS__;
    });
    expect(commandsInvoked).not.toContain("get_audio_file_path");
    expect(commandsInvoked).not.toContain("retry_history_entry_transcription");
    expect(commandsInvoked).not.toContain("learn_custom_words_from_correction");
  });

  test("language guard toast can paste the last transcript anyway", async ({
    page,
  }) => {
    await installTauriMocks(page);
    await page.goto("/");
    await expect(page.getByTitle("General")).toBeVisible();

    await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_EMIT_EVENT__: (
          event: string,
          payload?: unknown,
        ) => void;
      };
      win.__VERBATIM_TEST_EMIT_EVENT__("language-guard-blocked", {
        locked_language: "en",
        preview: "مرحبا بالعالم",
      });
    });

    await expect(page.getByText("Wrong Language Detected")).toBeVisible();
    await page.getByRole("button", { name: "Paste anyway" }).click();

    const commands = await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_COMMANDS__: string[];
      };
      return win.__VERBATIM_TEST_COMMANDS__;
    });
    expect(commands).toContain("paste_last_transcript");
  });

  test("paste failure toast can copy the last transcript again", async ({
    page,
  }) => {
    await installTauriMocks(page);
    await page.goto("/");
    await expect(page.getByTitle("General")).toBeVisible();

    await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_EMIT_EVENT__: (
          event: string,
          payload?: unknown,
        ) => void;
      };
      win.__VERBATIM_TEST_EMIT_EVENT__("paste-error");
    });

    await expect(page.getByText("Failed to Paste Text")).toBeVisible();
    await page.getByRole("button", { name: "Copy again" }).click();

    const commands = await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_COMMANDS__: string[];
      };
      return win.__VERBATIM_TEST_COMMANDS__;
    });
    expect(commands).toContain("copy_last_transcript");
  });

  test("target-changed paste recovery can paste into the current field", async ({
    page,
  }) => {
    await installTauriMocks(page);
    await page.goto("/");
    await expect(page.getByTitle("General")).toBeVisible();

    await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_EMIT_EVENT__: (
          event: string,
          payload?: unknown,
        ) => void;
      };
      win.__VERBATIM_TEST_EMIT_EVENT__("paste-error", {
        reason: "target_changed",
        copied: false,
        paste_here_available: true,
      });
    });

    await expect(page.getByText("Insertion Blocked")).toBeVisible();
    await page.getByRole("button", { name: "Paste here" }).click();

    const commands = await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_COMMANDS__: string[];
      };
      return win.__VERBATIM_TEST_COMMANDS__;
    });
    expect(commands).toContain("paste_last_transcript");
  });

  test("language guard paste-error payload does not duplicate recovery toast", async ({
    page,
  }) => {
    await installTauriMocks(page);
    await page.goto("/");
    await expect(page.getByTitle("General")).toBeVisible();

    await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_EMIT_EVENT__: (
          event: string,
          payload?: unknown,
        ) => void;
      };
      win.__VERBATIM_TEST_EMIT_EVENT__("paste-error", {
        reason: "language_guard",
        copied: true,
        paste_here_available: false,
      });
    });

    await expect(page.getByText("Failed to Paste Text")).toHaveCount(0);
  });

  test("recording overlay shows and cycles dictation language mode", async ({
    page,
  }) => {
    await installTauriMocks(page);
    await page.goto("/src/overlay/index.html");

    await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_EMIT_EVENT__: (
          event: string,
          payload?: unknown,
        ) => void;
      };
      win.__VERBATIM_TEST_EMIT_EVENT__("show-overlay", "recording");
    });

    const languageButton = page.getByRole("button", {
      name: "Change dictation language",
    });
    await expect(languageButton).toBeVisible();
    await expect(languageButton).toHaveText("Auto");
    await expectTextFits(page, ".language-mode-chip");

    await languageButton.click();
    await expect(languageButton).toHaveText("FR");
    await expectTextFits(page, ".language-mode-chip");

    const commands = await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_COMMANDS__: string[];
      };
      return win.__VERBATIM_TEST_COMMANDS__;
    });
    expect(commands).toContain("change_dictation_language_mode_setting");
  });

  test("advanced settings can enable docked pill mode", async ({ page }) => {
    await installTauriMocks(page);
    await page.goto("/");
    await expect(page.getByTitle("General")).toBeVisible();

    await page.getByTitle("Advanced").click();
    await settingRow(page, "Docked pill")
      .getByRole("checkbox")
      .check({ force: true });

    const commands = await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_COMMANDS__: string[];
      };
      return win.__VERBATIM_TEST_COMMANDS__;
    });
    expect(commands).toContain("change_docked_pill_setting");
  });

  test("recording overlay can stay docked collapsed and expand on click", async ({
    page,
  }) => {
    await installTauriMocks(page);
    await page.goto("/src/overlay/index.html");

    await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_EMIT_EVENT__: (
          event: string,
          payload?: unknown,
        ) => void;
      };
      win.__VERBATIM_TEST_EMIT_EVENT__("show-docked-overlay");
    });

    await expect(page.getByTestId("recording-overlay")).toHaveClass(
      /docked-collapsed/,
    );
    await expect(page.getByTestId("recording-overlay")).toHaveCSS(
      "width",
      "44px",
    );
    await expect(
      page.getByRole("button", { name: "Expand pill" }),
    ).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Change dictation language" }),
    ).toBeHidden();

    let overlayGeometryInvokes = await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_INVOKES__: Array<{
          cmd: string;
          args?: Record<string, unknown>;
        }>;
      };
      return win.__VERBATIM_TEST_INVOKES__.filter(
        (invoke) => invoke.cmd === "set_recording_overlay_expanded",
      );
    });
    expect(overlayGeometryInvokes).toContainEqual(
      expect.objectContaining({
        args: expect.objectContaining({ expanded: false }),
      }),
    );

    await page.getByRole("button", { name: "Expand pill" }).click();
    await expect(page.getByTestId("recording-overlay")).toHaveClass(
      /docked-expanded/,
    );
    await expect(
      page.getByRole("button", { name: "Change dictation language" }),
    ).toBeVisible();

    overlayGeometryInvokes = await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_INVOKES__: Array<{
          cmd: string;
          args?: Record<string, unknown>;
        }>;
      };
      return win.__VERBATIM_TEST_INVOKES__.filter(
        (invoke) => invoke.cmd === "set_recording_overlay_expanded",
      );
    });
    expect(overlayGeometryInvokes).toContainEqual(
      expect.objectContaining({
        args: expect.objectContaining({ expanded: true }),
      }),
    );
  });

  test("docked pill stays collapsed on hover and expands only on click", async ({
    page,
  }) => {
    await installTauriMocks(page);
    await page.goto("/src/overlay/index.html");

    await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_EMIT_EVENT__: (
          event: string,
          payload?: unknown,
        ) => void;
      };
      win.__VERBATIM_TEST_EMIT_EVENT__("show-docked-overlay");
    });

    const overlay = page.getByTestId("recording-overlay");
    const expandButton = page.getByRole("button", { name: "Expand pill" });

    await expect(overlay).toHaveClass(/docked-collapsed/);
    await expandButton.hover();
    await expect(overlay).toHaveClass(/docked-collapsed/);
    await expect(overlay).toHaveCSS("width", "44px");

    await expandButton.click();
    await expect(overlay).toHaveClass(/docked-expanded/);
  });

  test("docked pill initializes visible when already enabled", async ({
    page,
  }) => {
    await installTauriMocks(page, {
      docked_pill_enabled: true,
      overlay_position: "bottom",
    });
    await page.goto("/src/overlay/index.html");

    const overlay = page.getByTestId("recording-overlay");
    await expect(overlay).toHaveClass(/fade-in/);
    await expect(overlay).toHaveClass(/docked-collapsed/);
    await expect(
      page.getByRole("button", { name: "Expand pill" }),
    ).toBeVisible();
  });

  test("expanded docked pill exposes recovery and settings actions", async ({
    page,
  }) => {
    await installTauriMocks(page);
    await page.goto("/src/overlay/index.html");

    await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_EMIT_EVENT__: (
          event: string,
          payload?: unknown,
        ) => void;
      };
      win.__VERBATIM_TEST_EMIT_EVENT__("show-docked-overlay");
    });

    await page.getByRole("button", { name: "Expand pill" }).click();
    await page.getByRole("button", { name: "Copy last transcript" }).click();
    await page.getByRole("button", { name: "Paste last transcript" }).click();
    await page.getByRole("button", { name: "Open settings" }).click();
    await page.getByRole("button", { name: "Review dictionary" }).click();

    const result = await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_COMMANDS__: string[];
        __VERBATIM_TEST_EMITTED_EVENTS__: string[];
      };
      return {
        commands: win.__VERBATIM_TEST_COMMANDS__,
        events: win.__VERBATIM_TEST_EMITTED_EVENTS__,
      };
    });
    expect(result.commands).toContain("copy_last_transcript");
    expect(result.commands).toContain("paste_last_transcript");
    expect(result.commands).toContain("show_main_window_command");
    expect(result.events).toContain("open-dictionary-settings");
  });

  test("docked pill shows learned dictionary entries with undo and review", async ({
    page,
  }) => {
    await installTauriMocks(page);
    await page.goto("/src/overlay/index.html");

    await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_EMIT_EVENT__: (
          event: string,
          payload?: unknown,
        ) => void;
      };
      win.__VERBATIM_TEST_EMIT_EVENT__("show-docked-overlay");
    });
    await expect(page.getByTestId("recording-overlay")).toHaveClass(
      /docked-collapsed/,
    );

    await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_LEARN_ENTRIES__: (
          entries: Array<Record<string, unknown>>,
        ) => void;
      };
      win.__VERBATIM_TEST_LEARN_ENTRIES__([
        {
          id: "dict_test_kulaib",
          phrase: "Abdullah al Kulaib",
          replacement_of: "abdullah al kulaib",
          source: "auto_learned",
          priority: "normal",
          created_at_ms: 1,
          updated_at_ms: 1,
        },
      ]);
    });

    const overlay = page.getByTestId("recording-overlay");
    await expect(overlay).toHaveClass(/docked-expanded/);
    await expect(overlay).toContainText("Added to dictionary");
    await expect(overlay).toContainText("Abdullah al Kulaib");

    await page.getByRole("button", { name: "Review dictionary" }).click();
    await page.getByRole("button", { name: "Undo learned word" }).click();

    const result = await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_COMMANDS__: string[];
        __VERBATIM_TEST_EMITTED_EVENTS__: string[];
      };
      return {
        commands: win.__VERBATIM_TEST_COMMANDS__,
        events: win.__VERBATIM_TEST_EMITTED_EVENTS__,
      };
    });
    expect(result.commands).toContain("undo_dictionary_entries");
    expect(result.events).toContain("open-dictionary-settings");
    await expect(overlay).not.toContainText("Abdullah al Kulaib");
  });

  test("main app can open dictionary settings from pill event", async ({
    page,
  }) => {
    await installTauriMocks(page);
    await page.goto("/");
    await expect(page.getByTitle("General")).toBeVisible();

    await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_EMIT_EVENT__: (
          event: string,
          payload?: unknown,
        ) => void;
      };
      win.__VERBATIM_TEST_EMIT_EVENT__("open-dictionary-settings");
    });

    await expect(
      page.getByRole("heading", { name: "Dictionary" }),
    ).toBeVisible();
  });

  test("expanded docked pill opens a compact language picker", async ({
    page,
  }) => {
    await installTauriMocks(page);
    await page.goto("/src/overlay/index.html");

    await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_EMIT_EVENT__: (
          event: string,
          payload?: unknown,
        ) => void;
      };
      win.__VERBATIM_TEST_EMIT_EVENT__("show-docked-overlay");
    });

    await page.getByRole("button", { name: "Expand pill" }).click();
    await page
      .getByRole("button", { name: "Change dictation language" })
      .click();

    const picker = page.getByRole("menu", { name: "Dictation language" });
    await expect(picker).toBeVisible();
    for (const option of ["Auto", "FR", "DE", "JA", "FR+2"]) {
      await expect(
        picker.getByRole("menuitemradio", { name: option, exact: true }),
      ).toBeVisible();
    }

    await picker
      .getByRole("menuitemradio", { name: "DE", exact: true })
      .click();
    await expect(
      page.getByRole("button", { name: "Change dictation language" }),
    ).toHaveText("DE");

    const invokes = await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_INVOKES__: Array<{
          cmd: string;
          args?: Record<string, unknown>;
        }>;
      };
      return win.__VERBATIM_TEST_INVOKES__;
    });
    expect(invokes).toContainEqual(
      expect.objectContaining({
        cmd: "change_dictation_language_mode_setting",
        args: expect.objectContaining({
          mode: "single",
          selectedLanguage: "de",
          languages: ["fr", "de", "ja"],
        }),
      }),
    );
  });

  test("transient overlay exposes state changes through a live status region", async ({
    page,
  }) => {
    await installTauriMocks(page);
    await page.goto("/src/overlay/index.html");

    const emitOverlayState = async (state: string) => {
      await page.evaluate((nextState) => {
        const win = window as typeof window & {
          __VERBATIM_TEST_EMIT_EVENT__: (
            event: string,
            payload?: unknown,
          ) => void;
        };
        win.__VERBATIM_TEST_EMIT_EVENT__("show-overlay", nextState);
      }, state);
    };
    const status = page.getByRole("status");

    await emitOverlayState("recording");
    await expect(status).toHaveAttribute("aria-live", "polite");
    await expect(status).toHaveAccessibleName("Recording");

    for (const [state, label] of [
      ["processing", "Processing..."],
      ["inserted", "Inserted"],
      ["copied", "Copied"],
      ["cancelled", "Cancelled"],
      ["paste_failed", "Paste failed"],
    ]) {
      await emitOverlayState(state);
      await expect(status).toHaveAccessibleName(label);
    }
  });

  test("docked pill renders typed terminal and recovery states", async ({
    page,
  }) => {
    await installTauriMocks(page);
    await page.goto("/src/overlay/index.html");

    await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_EMIT_EVENT__: (
          event: string,
          payload?: unknown,
        ) => void;
      };
      win.__VERBATIM_TEST_EMIT_EVENT__("show-docked-overlay");
    });
    await expect(page.getByTestId("recording-overlay")).toHaveClass(
      /docked-collapsed/,
    );
    await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_EMIT_EVENT__: (
          event: string,
          payload?: unknown,
        ) => void;
      };
      win.__VERBATIM_TEST_EMIT_EVENT__("overlay-state-changed", "inserted");
    });

    const overlay = page.getByTestId("recording-overlay");
    await expect(overlay).toHaveClass(/docked-expanded/);
    await expect(overlay).toContainText("Inserted");

    await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_EMIT_EVENT__: (
          event: string,
          payload?: unknown,
        ) => void;
      };
      win.__VERBATIM_TEST_EMIT_EVENT__("overlay-state-changed", "copied");
    });
    await expect(overlay).toContainText("Copied");

    await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_EMIT_EVENT__: (
          event: string,
          payload?: unknown,
        ) => void;
      };
      win.__VERBATIM_TEST_EMIT_EVENT__("overlay-state-changed", "silence");
    });
    await expect(overlay).toContainText("No speech detected");
    await expect(overlay).toHaveAttribute("data-state", "silence");
    await expect(overlay.getByText("No speech detected")).toHaveCSS(
      "color",
      "rgb(255, 255, 255)",
    );

    await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_EMIT_EVENT__: (
          event: string,
          payload?: unknown,
        ) => void;
      };
      win.__VERBATIM_TEST_EMIT_EVENT__("overlay-state-changed", "mic_failed");
    });
    await expect(overlay).toContainText("Microphone issue");
    await expect(overlay).toHaveAttribute("data-state", "mic_failed");
    await expect(overlay.getByText("Microphone issue")).toHaveCSS(
      "color",
      "rgb(255, 255, 255)",
    );
    await expect(page.getByRole("button", { name: "Try again" })).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Select microphone" }),
    ).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Cancel recording" }),
    ).toBeVisible();

    await page.getByRole("button", { name: "Try again" }).click();
    await expect(
      page.getByRole("button", { name: "Change dictation language" }),
    ).toBeVisible();
    await expect(page.getByRole("button", { name: "Try again" })).toBeHidden();
    await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_EMIT_EVENT__: (
          event: string,
          payload?: unknown,
        ) => void;
      };
      win.__VERBATIM_TEST_EMIT_EVENT__("overlay-state-changed", "mic_failed");
    });
    await page.getByRole("button", { name: "Select microphone" }).click();

    const result = await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_COMMANDS__: string[];
        __VERBATIM_TEST_EMITTED_EVENTS__: string[];
      };
      return {
        commands: win.__VERBATIM_TEST_COMMANDS__,
        events: win.__VERBATIM_TEST_EMITTED_EVENTS__,
      };
    });
    expect(result.commands).toContain("retry_current_recording");
    expect(result.commands).toContain("show_main_window_command");
    expect(result.events).toContain("open-general-settings");
  });

  test("docked pill surfaces language guard recovery", async ({ page }) => {
    await installTauriMocks(page);
    await page.goto("/src/overlay/index.html");

    await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_EMIT_EVENT__: (
          event: string,
          payload?: unknown,
        ) => void;
      };
      win.__VERBATIM_TEST_EMIT_EVENT__("show-docked-overlay");
    });
    await expect(page.getByTestId("recording-overlay")).toHaveClass(
      /docked-collapsed/,
    );

    await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_EMIT_EVENT__: (
          event: string,
          payload?: unknown,
        ) => void;
      };
      win.__VERBATIM_TEST_EMIT_EVENT__("language-guard-blocked", {
        locked_language: "ar",
        preview: "hello world",
      });
    });

    const overlay = page.getByTestId("recording-overlay");
    await expect(overlay).toHaveClass(/docked-expanded/);
    await expect(overlay).toContainText("Copied");
    await expect(
      page.getByRole("button", { name: "Paste last transcript" }),
    ).toBeVisible();
  });

  test("docked pill surfaces transform copied recovery", async ({ page }) => {
    await installTauriMocks(page);
    await page.goto("/src/overlay/index.html");

    await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_EMIT_EVENT__: (
          event: string,
          payload?: unknown,
        ) => void;
      };
      win.__VERBATIM_TEST_EMIT_EVENT__("show-docked-overlay");
    });
    await expect(page.getByTestId("recording-overlay")).toHaveClass(
      /docked-collapsed/,
    );

    await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_EMIT_EVENT__: (
          event: string,
          payload?: unknown,
        ) => void;
      };
      win.__VERBATIM_TEST_EMIT_EVENT__("transform-recovery-copied");
    });

    const overlay = page.getByTestId("recording-overlay");
    await expect(overlay).toHaveClass(/docked-expanded/);
    await expect(overlay).toHaveAttribute("data-state", "transform_copied");
    await expect(overlay).toContainText("Transform copied");
    await expect(
      page.getByRole("button", { name: "Copy transform result" }),
    ).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Open settings" }),
    ).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Paste last transcript" }),
    ).toBeHidden();

    await page.getByRole("button", { name: "Copy transform result" }).click();
    await page.getByRole("button", { name: "Open settings" }).click();

    const result = await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_COMMANDS__: string[];
      };
      return win.__VERBATIM_TEST_COMMANDS__;
    });
    expect(result).toContain("copy_last_transform_result");
    expect(result).toContain("show_main_window_command");
  });

  test("docked pill keeps language guard recovery after docked reset", async ({
    page,
  }) => {
    await installTauriMocks(page, {
      docked_pill_enabled: true,
      overlay_position: "bottom",
    });
    await page.goto("/src/overlay/index.html");

    await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_EMIT_EVENT__: (
          event: string,
          payload?: unknown,
        ) => void;
      };
      win.__VERBATIM_TEST_EMIT_EVENT__("show-docked-overlay");
      win.__VERBATIM_TEST_EMIT_EVENT__("language-guard-blocked", {
        locked_language: "ar",
        preview: "hello world",
      });
      win.__VERBATIM_TEST_EMIT_EVENT__("show-docked-overlay");
    });

    const overlay = page.getByTestId("recording-overlay");
    await expect(overlay).toHaveClass(/docked-expanded/);
    await expect(overlay).toHaveAttribute("data-state", "copied");
    await expect(
      page.getByRole("button", { name: "Paste last transcript" }),
    ).toBeVisible();
  });

  test("docked pill keeps paste failure recovery after docked reset", async ({
    page,
  }) => {
    await installTauriMocks(page, {
      docked_pill_enabled: true,
      overlay_position: "bottom",
    });
    await page.goto("/src/overlay/index.html");

    await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_EMIT_EVENT__: (
          event: string,
          payload?: unknown,
        ) => void;
      };
      win.__VERBATIM_TEST_EMIT_EVENT__("show-docked-overlay");
      win.__VERBATIM_TEST_EMIT_EVENT__("paste-error");
      win.__VERBATIM_TEST_EMIT_EVENT__("show-docked-overlay");
    });

    const overlay = page.getByTestId("recording-overlay");
    await expect(overlay).toHaveClass(/docked-expanded/);
    await expect(overlay).toHaveAttribute("data-state", "paste_failed");
    await expect(
      page.getByRole("button", { name: "Retry paste" }),
    ).toBeVisible();
  });

  test("docked pill expands to live recording bars when recording starts", async ({
    page,
  }) => {
    await installTauriMocks(page);
    await page.goto("/src/overlay/index.html");

    await page.evaluate(async () => {
      const win = window as typeof window & {
        __TAURI_INTERNALS__: {
          invoke: (
            cmd: string,
            args?: Record<string, unknown>,
          ) => Promise<void>;
        };
        __VERBATIM_TEST_EMIT_EVENT__: (
          event: string,
          payload?: unknown,
        ) => void;
      };
      await win.__TAURI_INTERNALS__.invoke("change_docked_pill_setting", {
        enabled: true,
      });
      win.__VERBATIM_TEST_EMIT_EVENT__("show-docked-overlay");
    });
    await expect(page.getByTestId("recording-overlay")).toHaveClass(
      /docked-collapsed/,
    );

    await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_EMIT_EVENT__: (
          event: string,
          payload?: unknown,
        ) => void;
      };
      win.__VERBATIM_TEST_EMIT_EVENT__("show-overlay", "recording");
      win.__VERBATIM_TEST_EMIT_EVENT__(
        "mic-level",
        [0.9, 0.7, 0.5, 0.3, 0.6, 0.8, 0.4, 0.2, 0.5],
      );
    });

    const overlay = page.getByTestId("recording-overlay");
    await expect(overlay).toHaveClass(/docked-expanded/);
    await expect(overlay.locator(".bars-container")).toBeVisible();
    await expect(overlay.locator(".bar")).toHaveCount(9);
    await expect(overlay).toHaveCSS("width", "320px");
  });

  test("transient overlay does not inherit docked mode when setting is off", async ({
    page,
  }) => {
    await installTauriMocks(page);
    await page.goto("/src/overlay/index.html");

    await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_EMIT_EVENT__: (
          event: string,
          payload?: unknown,
        ) => void;
      };
      win.__VERBATIM_TEST_EMIT_EVENT__("show-docked-overlay");
    });
    await expect(page.getByTestId("recording-overlay")).toHaveClass(
      /docked-collapsed/,
    );

    await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_TEST_EMIT_EVENT__: (
          event: string,
          payload?: unknown,
        ) => void;
      };
      win.__VERBATIM_TEST_EMIT_EVENT__("show-overlay", "recording");
    });

    await expect(page.getByTestId("recording-overlay")).not.toHaveClass(
      /docked-/,
    );
  });
});
