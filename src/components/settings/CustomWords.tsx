import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useSettings } from "../../hooks/useSettings";
import { Input } from "../ui/Input";
import { Button } from "../ui/Button";
import { SettingContainer } from "../ui/SettingContainer";
import { ToggleSwitch } from "../ui/ToggleSwitch";

interface CustomWordsProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const CustomWords: React.FC<CustomWordsProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();
    const [newWord, setNewWord] = useState("");
    const [recentlyLearnedWords, setRecentlyLearnedWords] = useState<string[]>(
      [],
    );
    const customWords = getSetting("custom_words") || [];
    const autoAddDictionaryWords =
      getSetting("auto_add_dictionary_words") || false;

    useEffect(() => {
      const handleLearnedWords = (event: Event) => {
        const learnedWords = (event as CustomEvent<string[]>).detail ?? [];
        setRecentlyLearnedWords(learnedWords);
      };

      window.addEventListener(
        "verbatim-custom-words-learned",
        handleLearnedWords,
      );
      return () =>
        window.removeEventListener(
          "verbatim-custom-words-learned",
          handleLearnedWords,
        );
    }, []);

    useEffect(() => {
      setRecentlyLearnedWords((words) =>
        words.filter((word) => customWords.includes(word)),
      );
    }, [customWords]);

    const handleAddWord = () => {
      const trimmedWord = newWord.trim();
      const sanitizedWord = trimmedWord.replace(/[<>"'&]/g, "");
      if (
        sanitizedWord &&
        !sanitizedWord.includes(" ") &&
        sanitizedWord.length <= 50
      ) {
        if (customWords.includes(sanitizedWord)) {
          toast.error(
            t("settings.advanced.customWords.duplicate", {
              word: sanitizedWord,
            }),
          );
          return;
        }
        updateSetting("custom_words", [...customWords, sanitizedWord]);
        setNewWord("");
      }
    };

    const handleRemoveWord = (wordToRemove: string) => {
      updateSetting(
        "custom_words",
        customWords.filter((word) => word !== wordToRemove),
      );
    };

    const handleKeyPress = (e: React.KeyboardEvent) => {
      if (e.key === "Enter") {
        e.preventDefault();
        handleAddWord();
      }
    };

    return (
      <>
        <SettingContainer
          title={t("settings.advanced.customWords.title")}
          description={t("settings.advanced.customWords.description")}
          descriptionMode={descriptionMode}
          grouped={grouped}
        >
          <div className="flex items-center gap-2">
            <Input
              type="text"
              className="max-w-40"
              value={newWord}
              onChange={(e) => setNewWord(e.target.value)}
              onKeyDown={handleKeyPress}
              placeholder={t("settings.advanced.customWords.placeholder")}
              variant="compact"
              disabled={isUpdating("custom_words")}
            />
            <Button
              onClick={handleAddWord}
              disabled={
                !newWord.trim() ||
                newWord.includes(" ") ||
                newWord.trim().length > 50 ||
                isUpdating("custom_words")
              }
              variant="primary"
              size="md"
            >
              {t("settings.advanced.customWords.add")}
            </Button>
          </div>
        </SettingContainer>
        <ToggleSwitch
          checked={autoAddDictionaryWords}
          onChange={(enabled) =>
            updateSetting("auto_add_dictionary_words", enabled)
          }
          isUpdating={isUpdating("auto_add_dictionary_words")}
          label={t("settings.advanced.customWords.autoAdd.label")}
          description={t("settings.advanced.customWords.autoAdd.description")}
          descriptionMode={descriptionMode}
          grouped={grouped}
        />
        {recentlyLearnedWords.length > 0 && (
          <div
            role="status"
            aria-live="polite"
            data-testid="custom-words-recently-learned"
            className={`px-4 p-2 ${grouped ? "" : "rounded-lg border border-logo-primary/30"} bg-logo-primary/10 text-sm`}
          >
            <div className="font-medium text-logo-primary">
              {t("settings.advanced.customWords.autoAdd.learnedTitle")}
            </div>
            <div>
              {t("settings.advanced.customWords.autoAdd.learnedDescription", {
                words: recentlyLearnedWords.join(", "),
              })}
            </div>
          </div>
        )}
        {customWords.length > 0 && (
          <div
            className={`px-4 p-2 ${grouped ? "" : "rounded-lg border border-mid-gray/20"} flex flex-wrap gap-1`}
          >
            {customWords.map((word) => {
              const recentlyLearned = recentlyLearnedWords.includes(word);
              return (
                <Button
                  key={word}
                  onClick={() => handleRemoveWord(word)}
                  disabled={isUpdating("custom_words")}
                  variant="secondary"
                  size="sm"
                  className={`inline-flex items-center gap-1 cursor-pointer ${
                    recentlyLearned
                      ? "border-logo-primary bg-logo-primary/15 text-logo-primary ring-1 ring-logo-primary/50"
                      : ""
                  }`}
                  data-recently-learned={recentlyLearned ? "true" : undefined}
                  aria-label={t("settings.advanced.customWords.remove", {
                    word,
                  })}
                >
                  <span>{word}</span>
                  <svg
                    className="w-3 h-3"
                    fill="none"
                    stroke="currentColor"
                    viewBox="0 0 24 24"
                  >
                    <path
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      strokeWidth={2}
                      d="M6 18L18 6M6 6l12 12"
                    />
                  </svg>
                </Button>
              );
            })}
          </div>
        )}
      </>
    );
  },
);
