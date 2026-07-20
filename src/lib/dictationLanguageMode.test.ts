import { describe, expect, test } from "bun:test";
import {
  getDictationLanguageMode,
  getDictationLanguageModeLabel,
  getDictationLanguagePickerOptions,
  getNextDictationLanguageSelection,
  getSettingsForDictationLanguageSelection,
} from "./dictationLanguageMode";

describe("dictation language mode", () => {
  test("maps any forced recognition language to single-language mode", () => {
    expect(
      getDictationLanguageMode({
        selectedLanguage: "fr",
        adaptiveLanguageShortlist: ["fr", "de"],
      }),
    ).toBe("single");
  });

  test("maps any explicit multilingual shortlist to multilingual mode", () => {
    expect(
      getDictationLanguageMode({
        dictationLanguageMode: "multilingual",
        selectedLanguage: "auto",
        adaptiveLanguageShortlist: ["ja", "en"],
      }),
    ).toBe("multilingual");
  });

  test("keeps explicit auto distinct from multilingual shortlist defaults", () => {
    expect(
      getDictationLanguageMode({
        dictationLanguageMode: "auto",
        selectedLanguage: "auto",
        adaptiveLanguageShortlist: ["en", "ar"],
      }),
    ).toBe("auto");
  });

  test("treats an explicit backend mode as authoritative over a legacy language lock", () => {
    expect(
      getDictationLanguageMode({
        dictationLanguageMode: "auto",
        selectedLanguage: "ar",
        adaptiveLanguageShortlist: ["ar", "en"],
      }),
    ).toBe("auto");
  });

  test("cycles through auto, each configured language, multilingual, then auto", () => {
    expect(
      getNextDictationLanguageSelection({
        dictationLanguageMode: "auto",
        selectedLanguage: "auto",
        adaptiveLanguageShortlist: ["fr", "de", "ja"],
      }),
    ).toEqual({
      dictationLanguageMode: "single",
      selectedLanguage: "fr",
      adaptiveLanguageShortlist: ["fr", "de", "ja"],
    });

    expect(
      getNextDictationLanguageSelection({
        dictationLanguageMode: "single",
        selectedLanguage: "fr",
        adaptiveLanguageShortlist: ["fr", "de", "ja"],
      }),
    ).toEqual({
      dictationLanguageMode: "single",
      selectedLanguage: "de",
      adaptiveLanguageShortlist: ["fr", "de", "ja"],
    });

    expect(
      getNextDictationLanguageSelection({
        dictationLanguageMode: "single",
        selectedLanguage: "ja",
        adaptiveLanguageShortlist: ["fr", "de", "ja"],
      }),
    ).toEqual({
      dictationLanguageMode: "multilingual",
      selectedLanguage: "auto",
      adaptiveLanguageShortlist: ["fr", "de", "ja"],
    });

    expect(
      getNextDictationLanguageSelection({
        dictationLanguageMode: "multilingual",
        selectedLanguage: "auto",
        adaptiveLanguageShortlist: ["fr", "de", "ja"],
      }),
    ).toEqual({
      dictationLanguageMode: "auto",
      selectedLanguage: "auto",
      adaptiveLanguageShortlist: ["fr", "de", "ja"],
    });
  });

  test("maps selections back to transcription settings without enabling translation", () => {
    expect(
      getSettingsForDictationLanguageSelection({
        dictationLanguageMode: "single",
        selectedLanguage: "de",
        adaptiveLanguageShortlist: ["fr", "de", "ja"],
      }),
    ).toEqual({
      dictationLanguageMode: "single",
      selectedLanguage: "de",
      adaptiveLanguageShortlist: ["fr", "de", "ja"],
    });

    expect(
      getSettingsForDictationLanguageSelection({
        dictationLanguageMode: "multilingual",
        selectedLanguage: "auto",
        adaptiveLanguageShortlist: ["fr", "de", "ja"],
      }),
    ).toEqual({
      dictationLanguageMode: "multilingual",
      selectedLanguage: "auto",
      adaptiveLanguageShortlist: ["fr", "de", "ja"],
    });

    expect(
      getSettingsForDictationLanguageSelection({
        dictationLanguageMode: "auto",
        selectedLanguage: "auto",
        adaptiveLanguageShortlist: ["fr", "de", "ja"],
      }),
    ).toEqual({
      dictationLanguageMode: "auto",
      selectedLanguage: "auto",
      adaptiveLanguageShortlist: ["fr", "de", "ja"],
    });
  });

  test("labels the pill from configured language codes instead of hardcoded languages", () => {
    expect(
      getDictationLanguageModeLabel({
        dictationLanguageMode: "single",
        selectedLanguage: "de",
        adaptiveLanguageShortlist: ["fr", "de", "ja"],
      }),
    ).toBe("DE");

    expect(
      getDictationLanguageModeLabel({
        dictationLanguageMode: "multilingual",
        selectedLanguage: "auto",
        adaptiveLanguageShortlist: ["fr", "de", "ja"],
      }),
    ).toBe("FR+2");

    expect(
      getDictationLanguageModeLabel({
        dictationLanguageMode: "auto",
        selectedLanguage: "auto",
        adaptiveLanguageShortlist: ["fr", "de", "ja"],
      }),
    ).toBe("Auto");
  });

  test("builds picker options for auto, every configured language, and multilingual mode", () => {
    expect(
      getDictationLanguagePickerOptions({
        dictationLanguageMode: "auto",
        selectedLanguage: "auto",
        adaptiveLanguageShortlist: ["fr", "de", "ja"],
      }),
    ).toEqual([
      {
        label: "Auto",
        selection: {
          dictationLanguageMode: "auto",
          selectedLanguage: "auto",
          adaptiveLanguageShortlist: ["fr", "de", "ja"],
        },
      },
      {
        label: "FR",
        selection: {
          dictationLanguageMode: "single",
          selectedLanguage: "fr",
          adaptiveLanguageShortlist: ["fr", "de", "ja"],
        },
      },
      {
        label: "DE",
        selection: {
          dictationLanguageMode: "single",
          selectedLanguage: "de",
          adaptiveLanguageShortlist: ["fr", "de", "ja"],
        },
      },
      {
        label: "JA",
        selection: {
          dictationLanguageMode: "single",
          selectedLanguage: "ja",
          adaptiveLanguageShortlist: ["fr", "de", "ja"],
        },
      },
      {
        label: "FR+2",
        selection: {
          dictationLanguageMode: "multilingual",
          selectedLanguage: "auto",
          adaptiveLanguageShortlist: ["fr", "de", "ja"],
        },
      },
    ]);
  });
});
