import React, { useState, useRef, useEffect, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { SettingContainer } from "../ui/SettingContainer";
import { ResetButton } from "../ui/ResetButton";
import { useSettings } from "../../hooks/useSettings";
import { LANGUAGES } from "../../lib/constants/languages";
import {
  getActiveLanguageSelection,
  getNextLanguageSelection,
} from "../../lib/languageSelection";

interface LanguageSelectorProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  supportedLanguages?: string[];
}

export const LanguageSelector: React.FC<LanguageSelectorProps> = ({
  descriptionMode = "tooltip",
  grouped = false,
  supportedLanguages,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, resetSetting, isUpdating } = useSettings();
  const [isOpen, setIsOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const dropdownRef = useRef<HTMLDivElement>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);

  const selectedLanguage = getSetting("selected_language") || "auto";
  const adaptiveLanguageShortlist =
    getSetting("adaptive_language_shortlist") ?? [];
  const isLanguageUpdating =
    isUpdating("selected_language") ||
    isUpdating("adaptive_language_shortlist");

  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (
        dropdownRef.current &&
        !dropdownRef.current.contains(event.target as Node)
      ) {
        setIsOpen(false);
        setSearchQuery("");
      }
    };

    document.addEventListener("mousedown", handleClickOutside);
    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
    };
  }, []);

  useEffect(() => {
    if (isOpen && searchInputRef.current) {
      searchInputRef.current.focus();
    }
  }, [isOpen]);

  const availableLanguages = useMemo(() => {
    if (!supportedLanguages || supportedLanguages.length === 0)
      return LANGUAGES.filter((lang) => lang.value !== "auto");
    return LANGUAGES.filter(
      (lang) =>
        lang.value !== "auto" && supportedLanguages.includes(lang.value),
    );
  }, [supportedLanguages]);

  const activeLanguageSelection = useMemo(
    () =>
      getActiveLanguageSelection({
        selectedLanguage,
        adaptiveLanguageShortlist,
      }),
    [selectedLanguage, adaptiveLanguageShortlist],
  );

  const filteredLanguages = useMemo(
    () =>
      availableLanguages.filter((language) =>
        language.label.toLowerCase().includes(searchQuery.toLowerCase()),
      ),
    [searchQuery, availableLanguages],
  );

  const getLanguageLabel = (languageCode: string) =>
    LANGUAGES.find((lang) => lang.value === languageCode)?.label ||
    languageCode;

  const selectedLanguageNames = activeLanguageSelection
    .map(getLanguageLabel)
    .join(", ");

  const selectedLanguageName =
    selectedLanguageNames || t("settings.general.language.auto");

  const handleLanguageToggle = async (languageCode: string) => {
    const nextSelection = getNextLanguageSelection({
      selectedLanguage,
      adaptiveLanguageShortlist,
      toggledLanguage: languageCode,
    });

    if (nextSelection.adaptiveLanguageShortlist.length === 0) {
      return;
    }

    await updateSetting(
      "adaptive_language_shortlist",
      nextSelection.adaptiveLanguageShortlist,
    );

    if (nextSelection.selectedLanguage !== selectedLanguage) {
      await updateSetting("selected_language", nextSelection.selectedLanguage);
    }
  };

  const handleReset = async () => {
    await resetSetting("selected_language");
  };

  const handleToggle = () => {
    if (isLanguageUpdating) return;
    setIsOpen(!isOpen);
  };

  const handleSearchChange = (event: React.ChangeEvent<HTMLInputElement>) => {
    setSearchQuery(event.target.value);
  };

  const handleKeyDown = (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter" && filteredLanguages.length > 0) {
      handleLanguageToggle(filteredLanguages[0].value);
    } else if (event.key === "Escape") {
      setIsOpen(false);
      setSearchQuery("");
    }
  };

  return (
    <SettingContainer
      title={t("settings.general.language.title")}
      description={t("settings.general.language.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
    >
      <div className="flex items-center space-x-1">
        <div className="relative" ref={dropdownRef}>
          <button
            type="button"
            className={`px-2 py-1 text-sm font-semibold bg-mid-gray/10 border border-mid-gray/80 rounded min-w-[200px] text-start flex items-center justify-between transition-all duration-150 ${
              isLanguageUpdating
                ? "opacity-50 cursor-not-allowed"
                : "hover:bg-accent/10 cursor-pointer hover:border-accent"
            }`}
            onClick={handleToggle}
            disabled={isLanguageUpdating}
          >
            <span className="truncate">{selectedLanguageName}</span>
            <svg
              className={`w-4 h-4 ms-2 transition-transform duration-200 ${
                isOpen ? "transform rotate-180" : ""
              }`}
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M19 9l-7 7-7-7"
              />
            </svg>
          </button>

          {isOpen && !isLanguageUpdating && (
            <div className="absolute top-full left-0 right-0 mt-1 bg-background border border-mid-gray/80 rounded shadow-lg z-50 max-h-60 overflow-hidden">
              {/* Search input */}
              <div className="p-2 border-b border-mid-gray/80">
                <input
                  ref={searchInputRef}
                  type="text"
                  value={searchQuery}
                  onChange={handleSearchChange}
                  onKeyDown={handleKeyDown}
                  placeholder={t("settings.general.language.searchPlaceholder")}
                  className="w-full px-2 py-1 text-sm bg-mid-gray/10 border border-mid-gray/40 rounded focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:border-accent"
                />
              </div>

              <div className="max-h-48 overflow-y-auto">
                {filteredLanguages.length === 0 ? (
                  <div className="px-2 py-2 text-sm text-mid-gray text-center">
                    {t("settings.general.language.noResults")}
                  </div>
                ) : (
                  filteredLanguages.map((language) => {
                    const isChecked = activeLanguageSelection.includes(
                      language.value,
                    );
                    const isOnlyChecked =
                      isChecked && activeLanguageSelection.length === 1;

                    return (
                      <button
                        key={language.value}
                        type="button"
                        className={`w-full px-2 py-1 text-sm text-start hover:bg-accent/10 transition-colors duration-150 ${
                          isChecked
                            ? "bg-accent/15 text-text font-medium"
                            : ""
                        } ${isOnlyChecked ? "cursor-default" : ""}`}
                        onClick={() => handleLanguageToggle(language.value)}
                        disabled={isOnlyChecked}
                      >
                        <div className="flex items-center gap-2">
                          <input
                            type="checkbox"
                            readOnly
                            checked={isChecked}
                            className="h-3.5 w-3.5 accent-accent"
                          />
                          <span className="truncate">{language.label}</span>
                        </div>
                      </button>
                    );
                  })
                )}
              </div>
            </div>
          )}
        </div>
        <ResetButton onClick={handleReset} disabled={isLanguageUpdating} />
      </div>
      {isLanguageUpdating && (
        <div className="absolute inset-0 bg-mid-gray/10 rounded flex items-center justify-center">
          <div className="w-4 h-4 border-2 border-accent border-t-transparent rounded-full animate-spin"></div>
        </div>
      )}
    </SettingContainer>
  );
};
