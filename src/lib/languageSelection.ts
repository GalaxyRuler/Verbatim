interface LanguageSelectionInput {
  selectedLanguage?: string | null;
  adaptiveLanguageShortlist?: string[] | null;
}

interface ToggleLanguageInput extends LanguageSelectionInput {
  toggledLanguage: string;
}

interface LanguageSelectionOutput {
  selectedLanguage: string;
  adaptiveLanguageShortlist: string[];
}

export const normalizeLanguageCodes = (languages: string[]): string[] =>
  languages
    .map((language) => language.trim())
    .filter((language) => language.length > 0 && language !== "auto")
    .reduce<string[]>((acc, language) => {
      if (!acc.includes(language)) acc.push(language);
      return acc;
    }, []);

export const getActiveLanguageSelection = ({
  selectedLanguage,
  adaptiveLanguageShortlist,
}: LanguageSelectionInput): string[] => {
  if (selectedLanguage && selectedLanguage !== "auto") {
    return [selectedLanguage];
  }

  return normalizeLanguageCodes(adaptiveLanguageShortlist ?? []);
};

export const getNextLanguageSelection = ({
  selectedLanguage,
  adaptiveLanguageShortlist,
  toggledLanguage,
}: ToggleLanguageInput): LanguageSelectionOutput => {
  const current = getActiveLanguageSelection({
    selectedLanguage,
    adaptiveLanguageShortlist,
  });
  const next = current.includes(toggledLanguage)
    ? current.filter((language) => language !== toggledLanguage)
    : [...current, toggledLanguage];

  const adaptiveLanguages = normalizeLanguageCodes(next);
  const nextSelectedLanguage =
    adaptiveLanguages.length === 1 ? adaptiveLanguages[0] : "auto";

  return {
    selectedLanguage: nextSelectedLanguage,
    adaptiveLanguageShortlist: adaptiveLanguages,
  };
};
