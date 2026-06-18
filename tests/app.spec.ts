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
  history_limit: 100,
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
  osType: "windows" | "linux" | "macos" = "windows",
) => {
  await page.addInitScript(
    ({
      settings,
      profiles,
      models,
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
      let historyRows = [
        ...((initialHistoryEntries as Array<Record<string, unknown>>) ?? []),
      ];
      let nextDictionaryId = 1;
      let nextSnippetId = 1;
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
          switch (cmd) {
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
              return true;
            case "get_available_models":
              return models;
            case "get_current_model":
            case "get_transcription_model_status":
              return "small";
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
            case "get_available_output_devices":
              return [];
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
      models,
      localPostProcessingModels: localLlmModels,
      initialHistoryEntries: historyEntries,
      osType,
    },
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
    await expect(page.getByText("Adaptive Profiles")).toHaveCount(0);

    await settingRow(page, "Experimental Features")
      .getByRole("checkbox")
      .check({ force: true });

    const adaptiveRow = settingRow(page, "Adaptive Profiles");
    await expect(adaptiveRow).toBeVisible();
    await expect(page.getByText("Default Profile")).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Reprocess Last" }),
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

    await settingRow(page, "Experimental Features")
      .getByRole("checkbox")
      .check({ force: true });

    await expect(page.getByText("Context Awareness")).toBeVisible();
    await expect(page.getByText("Nearby Text")).toBeVisible();
    await expect(
      settingRow(page, "Nearby Text").getByRole("checkbox"),
    ).toBeDisabled();

    await settingRow(page, "Context Awareness")
      .getByRole("checkbox")
      .check({ force: true });
    await expect(
      settingRow(page, "Nearby Text").getByRole("checkbox"),
    ).toBeEnabled();
    await settingRow(page, "Nearby Text")
      .getByRole("checkbox")
      .check({ force: true });
    await expect(
      settingRow(page, "Nearby Text").getByRole("checkbox"),
    ).toBeChecked();

    await settingRow(page, "Context Awareness")
      .getByRole("checkbox")
      .uncheck({ force: true });
    await expect(
      settingRow(page, "Nearby Text").getByRole("checkbox"),
    ).toBeDisabled();
    await expect(
      settingRow(page, "Nearby Text").getByRole("checkbox"),
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
      page.getByRole("heading", { name: "Transform Selected Text" }),
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
      page.getByRole("heading", { name: "Transform Selected Text" }),
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

    const formattingRow = settingRow(page, "Smart Formatting");
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

    const formattingRow = settingRow(page, "Smart Formatting");
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
    await expect(page.getByTitle("Post Process")).toHaveCount(0);
    await expect(page.getByText("Managed Local Model")).toHaveCount(0);
  });

  test("local post-processing model can be downloaded selected and enabled", async ({
    page,
  }) => {
    await installTauriMocks(page, { post_process_enabled: true });
    await page.goto("/");

    await expect(page.getByTitle("General")).toBeVisible();
    await page.getByTitle("Post Process").click();
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
    await page.getByTitle("Post Process").click();

    await expect(page.getByText("Processing Engine")).toBeVisible();
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
    await expect(learnedStatus).toContainText("Added to dictionary");
    await expect(learnedStatus).toContainText("Added: Abdullah al Kulaib");

    await learnedStatus.getByRole("button", { name: "Undo" }).click();
    await expect(page.getByTestId("dictionary-entries-list")).not.toContainText(
      "Abdullah al Kulaib",
    );
  });

  test("advanced settings no longer owns dictionary management", async ({
    page,
  }) => {
    await installTauriMocks(page);
    await page.goto("/");

    await page.getByText("Advanced").click();
    await expect(page.getByText("Custom Words")).toHaveCount(0);
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

    await page.getByText("History").click();

    await expect(page.getByText("Polished transform result")).toBeVisible();
    await expect(page.getByText("Raw selected text")).toHaveCount(0);
    await expect(
      page.getByTitle("Copy transcription to clipboard"),
    ).toBeVisible();
    await expect(page.getByTitle("Save transcription")).toBeVisible();
    await expect(page.getByTitle("Delete entry")).toBeVisible();
    await expect(page.getByTitle("Learn dictionary correction")).toHaveCount(0);
    await expect(page.getByTitle("Re-transcribe")).toHaveCount(0);
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
    await settingRow(page, "Docked Pill")
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
