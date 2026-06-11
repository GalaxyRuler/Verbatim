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
  overlay_position: "top-right",
  debug_mode: false,
  log_level: "info",
  custom_words: [],
  model_unload_timeout: "never",
  word_correction_threshold: 0.8,
  history_limit: 100,
  recording_retention_period: "never",
  paste_method: "auto",
  clipboard_handling: "restore",
  auto_submit: false,
  auto_submit_key: "enter",
  post_process_enabled: false,
  post_process_provider_id: "openai",
  post_process_providers: [],
  post_process_api_keys: {},
  post_process_models: {},
  post_process_prompts: [],
  post_process_selected_prompt_id: null,
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
  adaptive_language_shortlist: ["en", "ar"],
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

const installTauriMocks = async (page: Page) => {
  await page.addInitScript(
    ({ settings, profiles, models }) => {
      let appSettings = { ...settings };
      const listeners = new Map<number, () => void>();
      let nextCallbackId = 1;
      const testWindow = window as typeof window & {
        __TAURI_INTERNALS__: any;
        __TAURI_OS_PLUGIN_INTERNALS__: any;
        __VERBATIM_TEST_COMMANDS__: string[];
      };
      testWindow.__VERBATIM_TEST_COMMANDS__ = [];

      testWindow.__TAURI_OS_PLUGIN_INTERNALS__ = {
        platform: "windows",
        os_type: "windows",
        family: "windows",
        version: "11",
        arch: "x86_64",
        exe_extension: "exe",
        eol: "\r\n",
      };

      testWindow.__TAURI_INTERNALS__ = {
        callbacks: {},
        convertFileSrc: (filePath: string) => filePath,
        invoke: async (cmd: string, args?: Record<string, unknown>) => {
          testWindow.__VERBATIM_TEST_COMMANDS__.push(cmd);
          switch (cmd) {
            case "get_default_settings":
            case "get_app_settings":
              return appSettings;
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
            case "change_adaptive_profiles_enabled_setting":
              appSettings = {
                ...appSettings,
                adaptive_profiles_enabled: Boolean(args?.enabled),
              };
              return null;
            default:
              return null;
          }
        },
        transformCallback: (callback: () => void) => {
          const id = nextCallbackId++;
          listeners.set(id, callback);
          return id;
        },
        unregisterCallback: (id: number) => {
          listeners.delete(id);
        },
        runCallback: (id: number) => {
          listeners.get(id)?.();
        },
        metadata: {
          currentWindow: { label: "main" },
          currentWebview: { label: "main" },
        },
      };
    },
    { settings: baseSettings, profiles: adaptiveProfiles, models },
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
    await expect(page.getByText("Verbatim", { exact: true })).toBeVisible();
    await page.getByText("Advanced").click();
    await expect(page.getByText("Adaptive Profiles")).toHaveCount(0);

    await settingRow(page, "Experimental Features")
      .getByRole("checkbox")
      .check({ force: true });

    const adaptiveRow = settingRow(page, "Adaptive Profiles");
    await expect(adaptiveRow).toBeVisible();
    await expect(page.getByText("Language Shortlist")).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Reprocess Last" }),
    ).toBeVisible();

    const adaptiveToggle = adaptiveRow.getByRole("checkbox");
    await expect(adaptiveToggle).not.toBeChecked();
    await adaptiveToggle.check({ force: true });
    await expect(adaptiveToggle).toBeChecked();
  });
});
