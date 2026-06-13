import { normalizeLanguageCodes } from "./languageSelection";

export type DictationLanguageMode = "auto" | "single" | "multilingual";

interface DictationLanguageSettings {
  dictationLanguageMode?: DictationLanguageMode | null;
  selectedLanguage?: string | null;
  adaptiveLanguageShortlist?: string[] | null;
}

export interface DictationLanguageSelection {
  dictationLanguageMode: DictationLanguageMode;
  selectedLanguage: string;
  adaptiveLanguageShortlist: string[];
}

const defaultShortlist = ["en", "ar"];

const shortlistOrDefault = (languages?: string[] | null): string[] => {
  const normalized = normalizeLanguageCodes(languages ?? []);
  return normalized.length > 0 ? normalized : defaultShortlist;
};

const upperCode = (language: string): string => language.toUpperCase();

export const getDictationLanguageMode = ({
  dictationLanguageMode,
  selectedLanguage,
  adaptiveLanguageShortlist,
}: DictationLanguageSettings): DictationLanguageMode => {
  if (dictationLanguageMode) return dictationLanguageMode;
  if (selectedLanguage && selectedLanguage !== "auto") return "single";

  const shortlist = normalizeLanguageCodes(adaptiveLanguageShortlist ?? []);
  return shortlist.length > 1 ? "multilingual" : "auto";
};

export const getSettingsForDictationLanguageSelection = ({
  dictationLanguageMode,
  selectedLanguage,
  adaptiveLanguageShortlist,
}: DictationLanguageSelection): DictationLanguageSelection => {
  const shortlist = shortlistOrDefault(adaptiveLanguageShortlist);

  if (dictationLanguageMode === "single") {
    const language =
      selectedLanguage && selectedLanguage !== "auto"
        ? selectedLanguage
        : shortlist[0];
    return {
      dictationLanguageMode: "single",
      selectedLanguage: language,
      adaptiveLanguageShortlist: shortlist.includes(language)
        ? shortlist
        : [language, ...shortlist],
    };
  }

  return {
    dictationLanguageMode,
    selectedLanguage: "auto",
    adaptiveLanguageShortlist: shortlist,
  };
};

export const getNextDictationLanguageSelection = ({
  dictationLanguageMode,
  selectedLanguage,
  adaptiveLanguageShortlist,
}: DictationLanguageSelection): DictationLanguageSelection => {
  const shortlist = shortlistOrDefault(adaptiveLanguageShortlist);

  if (dictationLanguageMode === "auto") {
    return {
      dictationLanguageMode: "single",
      selectedLanguage: shortlist[0],
      adaptiveLanguageShortlist: shortlist,
    };
  }

  if (dictationLanguageMode === "single") {
    const currentIndex = shortlist.indexOf(selectedLanguage);
    const nextLanguage = shortlist[currentIndex + 1];
    if (nextLanguage) {
      return {
        dictationLanguageMode: "single",
        selectedLanguage: nextLanguage,
        adaptiveLanguageShortlist: shortlist,
      };
    }

    return {
      dictationLanguageMode: shortlist.length > 1 ? "multilingual" : "auto",
      selectedLanguage: "auto",
      adaptiveLanguageShortlist: shortlist,
    };
  }

  return {
    dictationLanguageMode: "auto",
    selectedLanguage: "auto",
    adaptiveLanguageShortlist: shortlist,
  };
};

export const getDictationLanguageModeLabel = ({
  dictationLanguageMode,
  selectedLanguage,
  adaptiveLanguageShortlist,
}: DictationLanguageSelection): string => {
  if (dictationLanguageMode === "auto") return "Auto";
  if (dictationLanguageMode === "single") return upperCode(selectedLanguage);

  const shortlist = shortlistOrDefault(adaptiveLanguageShortlist);
  return shortlist.length <= 2
    ? shortlist.map(upperCode).join("+")
    : `${upperCode(shortlist[0])}+${shortlist.length - 1}`;
};
