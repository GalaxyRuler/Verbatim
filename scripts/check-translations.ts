import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

// Configuration
const LOCALES_DIR = path.join(__dirname, "..", "src", "i18n", "locales");
const REFERENCE_LANG = "en";

type TranslationData = Record<string, unknown>;

interface ValidationResult {
  valid: boolean;
  missing: string[][];
  extra: string[][];
  untranslated: string[][];
}

function getLanguages(): string[] {
  const entries = fs.readdirSync(LOCALES_DIR, { withFileTypes: true });
  return entries
    .filter((entry) => entry.isDirectory() && entry.name !== REFERENCE_LANG)
    .map((entry) => entry.name)
    .sort();
}

const LANGUAGES = getLanguages();
const RTL_LANGUAGES = new Set(["ar", "he"]);
const RTL_VALUE_CHECK_PREFIXES = [
  ["sidebar"],
  ["settings", "advanced", "translation"],
  ["settings", "advanced", "customWords", "autoAdd"],
  ["settings", "advanced", "adaptiveProfiles"],
  ["settings", "advanced", "dockedPill"],
  ["settings", "history"],
  ["settings", "dictionary"],
  ["settings", "formattingLevel"],
  ["settings", "snippets"],
  ["footer"],
  ["errors"],
  ["overlay"],
];
const RTL_VALUE_ALLOWLIST = new Set([
  "common.appName",
  "settings.dictionary.replacementPlaceholder",
  "settings.advanced.adaptiveProfiles.languages.placeholder",
  "settings.postProcessing.api.baseUrl.placeholder",
  "settings.postProcessing.api.apiKey.placeholder",
]);

// Colors for terminal output
const colors: Record<string, string> = {
  reset: "\x1b[0m",
  red: "\x1b[31m",
  green: "\x1b[32m",
  yellow: "\x1b[33m",
  blue: "\x1b[34m",
};

function colorize(text: string, color: string): string {
  return `${colors[color]}${text}${colors.reset}`;
}

function getAllKeyPaths(
  obj: TranslationData,
  prefix: string[] = [],
): string[][] {
  let paths: string[][] = [];
  for (const key in obj) {
    if (!Object.hasOwn(obj, key)) continue;

    const currentPath = prefix.concat([key]);
    const value = obj[key];

    if (typeof value === "object" && value !== null && !Array.isArray(value)) {
      paths = paths.concat(
        getAllKeyPaths(value as TranslationData, currentPath),
      );
    } else {
      paths.push(currentPath);
    }
  }
  return paths;
}

function hasKeyPath(obj: TranslationData, keyPath: string[]): boolean {
  let current: unknown = obj;
  for (const key of keyPath) {
    if (
      typeof current !== "object" ||
      current === null ||
      (current as Record<string, unknown>)[key] === undefined
    ) {
      return false;
    }
    current = (current as Record<string, unknown>)[key];
  }
  return true;
}

function getValueAtPath(obj: TranslationData, keyPath: string[]): unknown {
  let current: unknown = obj;
  for (const key of keyPath) {
    if (
      typeof current !== "object" ||
      current === null ||
      (current as Record<string, unknown>)[key] === undefined
    ) {
      return undefined;
    }
    current = (current as Record<string, unknown>)[key];
  }
  return current;
}

function startsWithPath(keyPath: string[], prefix: string[]): boolean {
  return (
    keyPath.length >= prefix.length &&
    prefix.every((segment, index) => keyPath[index] === segment)
  );
}

function shouldCheckRtlValue(keyPath: string[]): boolean {
  if (RTL_VALUE_ALLOWLIST.has(keyPath.join("."))) {
    return false;
  }

  return RTL_VALUE_CHECK_PREFIXES.some((prefix) =>
    startsWithPath(keyPath, prefix),
  );
}

function hasAsciiLetters(value: string): boolean {
  return /[A-Za-z]/.test(value);
}

function findUntranslatedRtlValues(
  langData: TranslationData,
  referenceData: TranslationData,
  referenceKeyPaths: string[][],
): string[][] {
  return referenceKeyPaths.filter((keyPath) => {
    if (!shouldCheckRtlValue(keyPath)) {
      return false;
    }

    const localizedValue = getValueAtPath(langData, keyPath);
    const referenceValue = getValueAtPath(referenceData, keyPath);

    return (
      typeof localizedValue === "string" &&
      typeof referenceValue === "string" &&
      localizedValue === referenceValue &&
      hasAsciiLetters(referenceValue)
    );
  });
}

function loadTranslationFile(lang: string): TranslationData | null {
  const filePath = path.join(LOCALES_DIR, lang, "translation.json");

  try {
    const content = fs.readFileSync(filePath, "utf8");
    return JSON.parse(content) as TranslationData;
  } catch (error) {
    console.error(colorize(`✗ Error loading ${lang}/translation.json:`, "red"));
    console.error(`  ${(error as Error).message}`);
    return null;
  }
}

function validateTranslations(): void {
  console.log(colorize("\n🌍 Translation Consistency Check\n", "blue"));

  // Load reference file
  console.log(`Loading reference language: ${REFERENCE_LANG}`);
  const referenceData = loadTranslationFile(REFERENCE_LANG);

  if (!referenceData) {
    console.error(
      colorize(`\n✗ Failed to load reference file (${REFERENCE_LANG})`, "red"),
    );
    process.exit(1);
  }

  // Get all key paths from reference
  const referenceKeyPaths = getAllKeyPaths(referenceData);
  console.log(`Reference has ${referenceKeyPaths.length} keys\n`);

  // Track validation results
  let hasErrors = false;
  const results: Record<string, ValidationResult> = {};

  // Validate each language
  for (const lang of LANGUAGES) {
    const langData = loadTranslationFile(lang);

    if (!langData) {
      hasErrors = true;
      results[lang] = {
        valid: false,
        missing: [],
        extra: [],
        untranslated: [],
      };
      continue;
    }

    // Find missing keys
    const missing = referenceKeyPaths.filter(
      (keyPath) => !hasKeyPath(langData, keyPath),
    );

    // Find extra keys (keys in language but not in reference)
    const langKeyPaths = getAllKeyPaths(langData);
    const extra = langKeyPaths.filter(
      (keyPath) => !hasKeyPath(referenceData, keyPath),
    );
    const untranslated = RTL_LANGUAGES.has(lang)
      ? findUntranslatedRtlValues(langData, referenceData, referenceKeyPaths)
      : [];

    results[lang] = {
      valid:
        missing.length === 0 && extra.length === 0 && untranslated.length === 0,
      missing,
      extra,
      untranslated,
    };

    if (missing.length > 0 || extra.length > 0 || untranslated.length > 0) {
      hasErrors = true;
    }
  }

  // Print results
  console.log(colorize("Results:", "blue"));
  console.log("─".repeat(60));

  for (const lang of LANGUAGES) {
    const result = results[lang];

    if (result.valid) {
      console.log(
        colorize(`✓ ${lang.toUpperCase()}: All keys present`, "green"),
      );
    } else {
      console.log(colorize(`✗ ${lang.toUpperCase()}: Issues found`, "red"));

      if (result.missing.length > 0) {
        console.log(
          colorize(`  Missing ${result.missing.length} keys:`, "yellow"),
        );
        result.missing.slice(0, 10).forEach((keyPath) => {
          console.log(`    - ${keyPath.join(".")}`);
        });
        if (result.missing.length > 10) {
          console.log(
            colorize(
              `    ... and ${result.missing.length - 10} more`,
              "yellow",
            ),
          );
        }
      }

      if (result.extra.length > 0) {
        console.log(
          colorize(
            `  Extra ${result.extra.length} keys (not in reference):`,
            "yellow",
          ),
        );
        result.extra.slice(0, 10).forEach((keyPath) => {
          console.log(`    - ${keyPath.join(".")}`);
        });
        if (result.extra.length > 10) {
          console.log(
            colorize(`    ... and ${result.extra.length - 10} more`, "yellow"),
          );
        }
      }

      if (result.untranslated.length > 0) {
        console.log(
          colorize(
            `  Untranslated fallback values ${result.untranslated.length} keys:`,
            "yellow",
          ),
        );
        result.untranslated.slice(0, 10).forEach((keyPath) => {
          console.log(`    - ${keyPath.join(".")}`);
        });
        if (result.untranslated.length > 10) {
          console.log(
            colorize(
              `    ... and ${result.untranslated.length - 10} more`,
              "yellow",
            ),
          );
        }
      }

      console.log("");
    }
  }

  console.log("─".repeat(60));

  // Summary
  const validCount = Object.values(results).filter((r) => r.valid).length;
  const totalCount = LANGUAGES.length;

  if (hasErrors) {
    console.log(
      colorize(
        `\n✗ Validation failed: ${validCount}/${totalCount} languages passed`,
        "red",
      ),
    );
    process.exit(1);
  } else {
    console.log(
      colorize(
        `\n✓ All ${totalCount} languages have complete translations!`,
        "green",
      ),
    );
    process.exit(0);
  }
}

// Run validation
validateTranslations();
