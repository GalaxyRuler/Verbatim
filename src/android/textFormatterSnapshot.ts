import type { DictionaryEntry, SnippetEntry } from "../bindings";

type AndroidTextFormatterLoadState = {
  dictionaryEntriesLoaded: boolean;
  snippetEntriesLoaded: boolean;
};

export function buildAndroidTextFormatterSnapshot(
  dictionaryEntries: DictionaryEntry[],
  snippetEntries: SnippetEntry[],
  loadState: AndroidTextFormatterLoadState,
): string | null {
  if (!loadState.dictionaryEntriesLoaded || !loadState.snippetEntriesLoaded) {
    return null;
  }

  return JSON.stringify({
    dictionary_entries: dictionaryEntries
      .filter((entry) => entry.active === true)
      .map((entry) => ({
        phrase: entry.phrase,
        replacement_of: entry.replacement_of ?? null,
        priority: entry.priority ?? "normal",
      })),
    snippets: snippetEntries.map((entry) => ({
      trigger: entry.trigger,
      content: entry.content,
    })),
  });
}
