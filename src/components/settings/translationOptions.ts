import type { Language } from "@/lib/constants/languages";

export type TranslationSupport =
  | { kind: "none" }
  | { kind: "english_only" }
  | { kind: "target_languages"; targetLanguages: string[] };

export const ENGLISH_TRANSLATION_SUPPORT: TranslationSupport = {
  kind: "english_only",
};

export function translationSupportFromModel(
  supportsTranslation: boolean,
): TranslationSupport {
  return supportsTranslation ? ENGLISH_TRANSLATION_SUPPORT : { kind: "none" };
}

export function isTranslationTargetSupported(
  support: TranslationSupport,
  targetLanguage: string,
): boolean {
  if (support.kind === "none") {
    return false;
  }

  if (support.kind === "english_only") {
    return targetLanguage === "en";
  }

  return support.targetLanguages.includes(targetLanguage);
}

export function translationTargetOptions(
  support: TranslationSupport,
  languages: Language[],
) {
  return languages
    .filter((language) => language.value !== "auto")
    .filter((language) => isTranslationTargetSupported(support, language.value))
    .map((language) => ({
      value: language.value,
      label: language.label,
    }));
}
