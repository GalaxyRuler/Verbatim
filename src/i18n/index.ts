import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import { locale } from "@tauri-apps/plugin-os";
import enTranslation from "./locales/en/translation.json";
import { LANGUAGE_METADATA } from "./languages";
import { commands } from "@/bindings";
import {
  getLanguageDirection,
  updateDocumentDirection,
  updateDocumentLanguage,
} from "@/lib/utils/rtl";

const localeLoaders = import.meta.glob<{ default: Record<string, unknown> }>([
  "./locales/*/translation.json",
  "!./locales/en/translation.json",
]);
const loadedLanguages = new Set<string>(["en"]);

const resources: Record<string, { translation: Record<string, unknown> }> = {
  en: { translation: enTranslation },
};

const supportedLocaleCodes = [
  "en",
  ...Object.keys(localeLoaders)
    .map((path) => path.match(/\.\/locales\/(.+)\/translation\.json/)?.[1])
    .filter((code): code is string => Boolean(code)),
];

// Build supported languages list from discovered locale paths + metadata.
export const SUPPORTED_LANGUAGES = supportedLocaleCodes
  .map((code) => {
    const meta = LANGUAGE_METADATA[code];
    if (!meta) {
      console.warn(`Missing metadata for locale "${code}" in languages.ts`);
      return { code, name: code, nativeName: code, priority: undefined };
    }
    return {
      code,
      name: meta.name,
      nativeName: meta.nativeName,
      priority: meta.priority,
    };
  })
  .sort((a, b) => {
    // Sort by priority first (lower = higher), then alphabetically
    if (a.priority !== undefined && b.priority !== undefined) {
      return a.priority - b.priority;
    }
    if (a.priority !== undefined) return -1;
    if (b.priority !== undefined) return 1;
    return a.name.localeCompare(b.name);
  });

export type SupportedLanguageCode = string;

const localePath = (code: string) => `./locales/${code}/translation.json`;

export const ensureLanguageLoaded = async (langCode: SupportedLanguageCode) => {
  if (loadedLanguages.has(langCode)) {
    return;
  }

  const loader = localeLoaders[localePath(langCode)];
  if (!loader) {
    console.warn(`No translation loader found for locale "${langCode}"`);
    return;
  }

  const module = await loader();
  i18n.addResourceBundle(langCode, "translation", module.default, true, true);
  loadedLanguages.add(langCode);
};

// Check if a language code is supported
const getSupportedLanguage = (
  langCode: string | null | undefined,
): SupportedLanguageCode | null => {
  if (!langCode) return null;
  const normalized = langCode.toLowerCase();
  // Try exact match first
  let supported = SUPPORTED_LANGUAGES.find(
    (lang) => lang.code.toLowerCase() === normalized,
  );
  if (!supported) {
    // Fall back to prefix match (language only, without region)
    const prefix = normalized.split("-")[0];
    supported = SUPPORTED_LANGUAGES.find(
      (lang) => lang.code.toLowerCase() === prefix,
    );
  }
  return supported ? supported.code : null;
};

export const changeAppLanguage = async (langCode: string) => {
  const supported = getSupportedLanguage(langCode);
  if (!supported) return;

  await ensureLanguageLoaded(supported);
  await i18n.changeLanguage(supported);
};

// Initialize i18n with English as default
// Language will be synced from settings after init
i18n.use(initReactI18next).init({
  resources,
  lng: "en",
  fallbackLng: "en",
  interpolation: {
    escapeValue: false, // React already escapes values
  },
  react: {
    useSuspense: false, // Disable suspense for SSR compatibility
  },
});

// Sync language from app settings
export const syncLanguageFromSettings = async () => {
  try {
    const result = await commands.getAppSettings();
    const settings = result.status === "ok" ? result.data : null;

    if (settings?.app_language) {
      const supported = getSupportedLanguage(settings.app_language);
      if (supported && supported !== i18n.language) {
        await changeAppLanguage(supported);
      }
    } else {
      // Fall back to system locale detection if no saved preference
      const systemLocale = await locale();
      const supported = getSupportedLanguage(systemLocale);
      if (supported && supported !== i18n.language) {
        await changeAppLanguage(supported);
      }
    }
  } catch (e) {
    console.warn("Failed to sync language from settings:", e);
  }
};

// Run language sync on init
syncLanguageFromSettings();

// Listen for language changes to update HTML dir and lang attributes
i18n.on("languageChanged", (lng) => {
  const dir = getLanguageDirection(lng);
  updateDocumentDirection(dir);
  updateDocumentLanguage(lng);
});

// Re-export RTL utilities for convenience
export { getLanguageDirection, isRTLLanguage } from "@/lib/utils/rtl";

export default i18n;
