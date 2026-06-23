import type { Page } from "@playwright/test";

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

const adaptiveProfiles = [
  {
    id: "default_clean",
    name: "Default Clean",
    description: "Clean dictation without changing meaning.",
    enabled: true,
  },
];

const baseSettings = {
  bindings: {},
  push_to_talk: false,
  audio_feedback: false,
  start_hidden: false,
  autostart_enabled: false,
  update_checks_enabled: false,
  selected_model: "small",
  always_on_microphone: false,
  selected_microphone: "Default",
  clamshell_microphone: "Default",
  selected_output_device: "Default",
  translate_to_english: false,
  selected_language: "auto",
  dictation_language_mode: "auto",
  docked_pill_enabled: false,
  overlay_position: "top-right",
  debug_mode: true,
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
  post_process_enabled: true,
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

interface A11yMockOptions {
  microphoneDenied?: boolean;
  settingsOverrides?: Partial<typeof baseSettings>;
}

export const installA11yTauriMocks = async (
  page: Page,
  options: A11yMockOptions = {},
) => {
  await page.addInitScript(
    ({ microphoneDenied, settings }) => {
      let appSettings = { ...settings };
      const callbacks = new Map<number, (payload?: unknown) => void>();
      const eventListeners = new Map<string, number[]>();
      let nextCallbackId = 1;
      const testWindow = window as typeof window & {
        __TAURI_INTERNALS__: any;
        __TAURI_EVENT_PLUGIN_INTERNALS__: any;
        __TAURI_OS_PLUGIN_INTERNALS__: any;
        __VERBATIM_A11Y_EMIT_EVENT__: (
          event: string,
          payload?: unknown,
        ) => void;
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
      const domainVersion = (id: string) =>
        (
          appSettings.settings_domain_versions as
            | Record<string, number>
            | undefined
        )?.[id] ?? 1;
      const settingsDocumentFromFlat = () => ({
        settings_schema_version: appSettings.settings_schema_version ?? 1,
        domains: {
          general: {
            version: domainVersion("general"),
            start_hidden: appSettings.start_hidden,
            autostart_enabled: appSettings.autostart_enabled,
            update_checks_enabled: appSettings.update_checks_enabled,
            overlay_position: appSettings.overlay_position,
            docked_pill_enabled: appSettings.docked_pill_enabled,
            app_language: appSettings.app_language,
            experimental_enabled: appSettings.experimental_enabled,
            show_tray_icon: appSettings.show_tray_icon,
            custom_words: appSettings.custom_words,
            dictionary_entries: appSettings.dictionary_entries,
            dictionary_auto_learn_suppressed:
              appSettings.dictionary_auto_learn_suppressed ?? [],
            auto_add_dictionary_words:
              appSettings.auto_add_dictionary_words ?? false,
            snippets: appSettings.snippets,
          },
          audio: {
            version: domainVersion("audio"),
            audio_feedback: appSettings.audio_feedback,
            audio_feedback_volume: appSettings.audio_feedback_volume ?? 1,
            sound_theme: appSettings.sound_theme ?? "marimba",
            always_on_microphone: appSettings.always_on_microphone,
            selected_microphone: appSettings.selected_microphone,
            clamshell_microphone: appSettings.clamshell_microphone,
            selected_output_device: appSettings.selected_output_device,
            mute_while_recording: appSettings.mute_while_recording,
            extra_recording_buffer_ms: appSettings.extra_recording_buffer_ms,
          },
          insertion: {
            version: domainVersion("insertion"),
            paste_method: appSettings.paste_method,
            clipboard_handling: appSettings.clipboard_handling,
            auto_submit: appSettings.auto_submit,
            auto_submit_key: appSettings.auto_submit_key,
            append_trailing_space: appSettings.append_trailing_space,
            paste_delay_ms: appSettings.paste_delay_ms,
            typing_tool: appSettings.typing_tool,
            external_script_path: appSettings.external_script_path,
          },
          privacy: {
            version: domainVersion("privacy"),
            history_enabled: appSettings.history_enabled,
            recordings_enabled: appSettings.recordings_enabled,
            history_limit: appSettings.history_limit,
            recording_retention_period: appSettings.recording_retention_period,
          },
          models: {
            version: domainVersion("models"),
            selected_model: appSettings.selected_model,
            model_unload_timeout: appSettings.model_unload_timeout,
            local_llm: appSettings.local_llm,
            whisper_accelerator: appSettings.whisper_accelerator,
            ort_accelerator: appSettings.ort_accelerator,
            whisper_gpu_device: appSettings.whisper_gpu_device,
          },
          post_processing: {
            version: domainVersion("post_processing"),
            post_process_enabled: appSettings.post_process_enabled,
            formatting_level: appSettings.formatting_level,
            post_process_provider_id: appSettings.post_process_provider_id,
            post_process_providers: appSettings.post_process_providers,
            post_process_api_keys: appSettings.post_process_api_keys,
            post_process_models: appSettings.post_process_models,
            post_process_prompts: appSettings.post_process_prompts,
            post_process_selected_prompt_id:
              appSettings.post_process_selected_prompt_id,
            translate_to_english: appSettings.translate_to_english,
            translation_enabled: appSettings.translation_enabled ?? false,
            translation_request: appSettings.translation_request ?? null,
            translation_provider_id:
              appSettings.translation_provider_id ?? null,
            translation_model_id: appSettings.translation_model_id ?? null,
          },
          diagnostics: {
            version: domainVersion("diagnostics"),
            debug_mode: appSettings.debug_mode,
            log_level: appSettings.log_level,
            lazy_stream_close: appSettings.lazy_stream_close,
          },
          adaptive: {
            version: domainVersion("adaptive"),
            selected_language: appSettings.selected_language,
            dictation_language_mode: appSettings.dictation_language_mode,
            word_correction_threshold: appSettings.word_correction_threshold,
            custom_filler_words: appSettings.custom_filler_words,
            adaptive_profiles_enabled: appSettings.adaptive_profiles_enabled,
            context_awareness_enabled: appSettings.context_awareness_enabled,
            context_nearby_text_enabled:
              appSettings.context_nearby_text_enabled,
            adaptive_language_shortlist:
              appSettings.adaptive_language_shortlist,
            adaptive_default_profile_id:
              appSettings.adaptive_default_profile_id,
            adaptive_profiles: appSettings.adaptive_profiles,
            adaptive_correction_memory_enabled:
              appSettings.adaptive_correction_memory_enabled,
            adaptive_private_app_patterns:
              appSettings.adaptive_private_app_patterns,
          },
          shortcuts: {
            version: domainVersion("shortcuts"),
            bindings: appSettings.bindings,
            push_to_talk: appSettings.push_to_talk,
            keyboard_implementation: appSettings.keyboard_implementation,
          },
        },
      });

      testWindow.__VERBATIM_A11Y_EMIT_EVENT__ = emitEvent;
      testWindow.__TAURI_OS_PLUGIN_INTERNALS__ = {
        platform: "windows",
        os_type: "windows",
        family: "windows",
        version: "11",
        arch: "x86_64",
        exe_extension: "exe",
        eol: "\r\n",
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
            case "get_startup_status":
              return { status: "ready" };
            case "get_default_settings":
            case "get_app_settings":
              return appSettings;
            case "get_default_settings_document":
            case "get_app_settings_document":
              return settingsDocumentFromFlat();
            case "has_any_models_available":
              return true;
            case "get_available_models":
              return models;
            case "get_current_model":
            case "get_transcription_model_status":
              return "small";
            case "get_model_info":
              return models[0];
            case "list_dictionary_entries":
            case "list_snippet_entries":
              return [];
            case "get_history_entries":
              return [];
            case "list_local_llm_models":
              return localLlmModels;
            case "get_adaptive_profiles":
              return adaptiveProfiles;
            case "get_available_microphones":
            case "get_available_output_devices":
              return [];
            case "get_available_accelerators":
              return {
                whisper: ["cpu"],
                ort: ["cpu"],
                gpu_devices: [],
              };
            case "get_windows_microphone_permission_status":
              return {
                supported: true,
                overall_access: microphoneDenied ? "denied" : "allowed",
                device_access: microphoneDenied ? "denied" : "allowed",
                app_access: microphoneDenied ? "denied" : "allowed",
                desktop_app_access: "allowed",
              };
            case "get_credential_store_status":
              return {
                available: true,
                platform: "windows",
                message: null,
                retained_legacy_api_key_count: 0,
              };
            case "get_private_session_status":
              return { enabled: false };
            case "get_app_dir_path":
              return "C:\\Users\\Admin\\AppData\\Roaming\\Verbatim";
            case "get_log_dir_path":
              return "C:\\Users\\Admin\\AppData\\Local\\Verbatim\\logs";
            case "check_custom_sounds":
              return { start: false, stop: false };
            case "is_recording":
            case "is_laptop":
              return false;
            case "change_post_process_enabled_setting":
              appSettings = {
                ...appSettings,
                post_process_enabled: Boolean(args?.enabled),
              };
              return null;
            case "change_dictation_language_mode_setting":
              appSettings = {
                ...appSettings,
                dictation_language_mode: args?.mode as string,
              };
              return null;
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
      microphoneDenied: options.microphoneDenied ?? false,
      settings: { ...baseSettings, ...options.settingsOverrides },
    },
  );
};
