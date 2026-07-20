import { describe, expect, test } from "bun:test";
import type { DictionaryEntry, SnippetEntry } from "../bindings";
import { buildAndroidTextFormatterSnapshot } from "./textFormatterSnapshot";

const dictionaryEntry = (
  overrides: Partial<DictionaryEntry> = {},
): DictionaryEntry => ({
  id: "dict-1",
  phrase: "Verbatim",
  replacement_of: "ver bait um",
  priority: "normal",
  active: true,
  ...overrides,
});

const snippetEntry = (overrides: Partial<SnippetEntry> = {}): SnippetEntry => ({
  id: "snippet-1",
  trigger: "/sig",
  content: "Sent from Verbatim",
  ...overrides,
});

describe("Android text formatter snapshot", () => {
  test("serializes only explicitly active dictionary entries", () => {
    const snapshot = buildAndroidTextFormatterSnapshot(
      [
        dictionaryEntry(),
        dictionaryEntry({
          id: "inactive",
          phrase: "quarantined",
          active: false,
        }),
        dictionaryEntry({
          id: "legacy",
          phrase: "missing active",
          active: undefined,
        }),
      ],
      [snippetEntry()],
      { dictionaryEntriesLoaded: true, snippetEntriesLoaded: true },
    );

    expect(snapshot).not.toBeNull();
    expect(JSON.parse(snapshot!)).toEqual({
      dictionary_entries: [
        {
          phrase: "Verbatim",
          replacement_of: "ver bait um",
          priority: "normal",
        },
      ],
      snippets: [{ trigger: "/sig", content: "Sent from Verbatim" }],
    });
  });

  test("does not build a snapshot before both stores have loaded", () => {
    expect(
      buildAndroidTextFormatterSnapshot([dictionaryEntry()], [snippetEntry()], {
        dictionaryEntriesLoaded: true,
        snippetEntriesLoaded: false,
      }),
    ).toBeNull();
  });

  test("does not build a snapshot when a load failed and stale data exists", () => {
    expect(
      buildAndroidTextFormatterSnapshot([dictionaryEntry()], [snippetEntry()], {
        dictionaryEntriesLoaded: false,
        snippetEntriesLoaded: true,
      }),
    ).toBeNull();
  });
});
