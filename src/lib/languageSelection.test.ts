import { describe, expect, test } from "bun:test";
import {
  getActiveLanguageSelection,
  getNextLanguageSelection,
} from "./languageSelection";

describe("language selection", () => {
  test("uses the forced transcription language when auto-detect is off", () => {
    expect(
      getActiveLanguageSelection({
        selectedLanguage: "ar",
        adaptiveLanguageShortlist: ["en", "ar"],
      }),
    ).toEqual(["ar"]);
  });

  test("adding a second language switches transcription to auto-detect with a shortlist", () => {
    expect(
      getNextLanguageSelection({
        selectedLanguage: "ar",
        adaptiveLanguageShortlist: ["en", "ar"],
        toggledLanguage: "en",
      }),
    ).toEqual({
      selectedLanguage: "auto",
      adaptiveLanguageShortlist: ["ar", "en"],
    });
  });

  test("one remaining selected language is forced for recognition accuracy", () => {
    expect(
      getNextLanguageSelection({
        selectedLanguage: "auto",
        adaptiveLanguageShortlist: ["en", "ar"],
        toggledLanguage: "en",
      }),
    ).toEqual({
      selectedLanguage: "ar",
      adaptiveLanguageShortlist: ["ar"],
    });
  });
});
