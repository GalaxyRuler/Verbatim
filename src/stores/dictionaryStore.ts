import { create } from "zustand";
import i18n from "@/i18n";
import {
  commands,
  type DictionaryDiagnostics,
  type DictionaryEntry,
  type DictionaryEntryInput,
  type DictionaryEntryUpdate,
  type LearnCandidate,
} from "@/bindings";

type DictionaryState = {
  entries: DictionaryEntry[];
  recentlyLearnedEntries: DictionaryEntry[];
  candidates: LearnCandidate[];
  diagnostics: DictionaryDiagnostics | null;
  isLoading: boolean;
  entriesLoaded: boolean;
  updatingIds: Set<string>;
  loadEntries: () => Promise<void>;
  addEntry: (input: DictionaryEntryInput) => Promise<DictionaryEntry>;
  updateEntry: (
    id: string,
    update: DictionaryEntryUpdate,
  ) => Promise<DictionaryEntry>;
  deleteEntry: (id: string) => Promise<void>;
  undoEntries: (ids: string[]) => Promise<void>;
  setRecentlyLearnedEntries: (entries: DictionaryEntry[]) => void;
  loadCandidates: () => Promise<void>;
  approveCandidate: (
    phrase: string,
    replacementOf: string | null,
  ) => Promise<void>;
  rejectCandidate: (phrase: string) => Promise<void>;
  setEntryActive: (id: string, active: boolean) => Promise<void>;
  loadDiagnostics: () => Promise<void>;
  resetDiagnostics: () => Promise<void>;
};

const sortEntries = (entries: DictionaryEntry[]) =>
  [...entries].sort((left, right) => {
    const leftStarred = left.priority === "starred";
    const rightStarred = right.priority === "starred";
    if (leftStarred !== rightStarred) {
      return leftStarred ? -1 : 1;
    }

    const updatedDiff = (right.updated_at_ms ?? 0) - (left.updated_at_ms ?? 0);
    if (updatedDiff !== 0) {
      return updatedDiff;
    }

    return left.phrase.localeCompare(right.phrase);
  });

const sortCandidates = (candidates: LearnCandidate[]) =>
  [...candidates].sort((left, right) => {
    const updatedDiff = (right.updated_at_ms ?? 0) - (left.updated_at_ms ?? 0);
    if (updatedDiff !== 0) {
      return updatedDiff;
    }

    return left.phrase.localeCompare(right.phrase);
  });

const unwrapResult = <T>(
  result: { status: "ok"; data: T } | { status: "error"; error: string },
) => {
  if (result.status === "error") {
    const message =
      result.error === "ambiguous_entry_id"
        ? i18n.t("settings.dictionary.errors.ambiguousEntryId")
        : result.error;
    throw new Error(message);
  }

  return result.data;
};

export const useDictionaryStore = create<DictionaryState>()((set, get) => ({
  entries: [],
  recentlyLearnedEntries: [],
  candidates: [],
  diagnostics: null,
  isLoading: false,
  entriesLoaded: false,
  updatingIds: new Set<string>(),

  loadEntries: async () => {
    set({ isLoading: true });
    try {
      const entries = unwrapResult(await commands.listDictionaryEntries());
      set({ entries: sortEntries(entries), entriesLoaded: true });
    } finally {
      set({ isLoading: false });
    }
  },

  addEntry: async (input) => {
    const entry = unwrapResult(await commands.addDictionaryEntry(input));
    set((state) => ({ entries: sortEntries([...state.entries, entry]) }));
    return entry;
  },

  updateEntry: async (id, update) => {
    set((state) => ({
      updatingIds: new Set([...state.updatingIds, id]),
    }));
    try {
      const entry = unwrapResult(
        await commands.updateDictionaryEntry(id, update),
      );
      set((state) => ({
        entries: sortEntries(
          state.entries.map((current) =>
            current.id === entry.id ? entry : current,
          ),
        ),
        recentlyLearnedEntries: state.recentlyLearnedEntries.map((current) =>
          current.id === entry.id ? entry : current,
        ),
      }));
      return entry;
    } finally {
      set((state) => {
        const updatingIds = new Set(state.updatingIds);
        updatingIds.delete(id);
        return { updatingIds };
      });
    }
  },

  deleteEntry: async (id) => {
    set((state) => ({
      updatingIds: new Set([...state.updatingIds, id]),
    }));
    try {
      unwrapResult(await commands.deleteDictionaryEntry(id));
      set((state) => ({
        entries: state.entries.filter((entry) => entry.id !== id),
        recentlyLearnedEntries: state.recentlyLearnedEntries.filter(
          (entry) => entry.id !== id,
        ),
      }));
    } finally {
      set((state) => {
        const updatingIds = new Set(state.updatingIds);
        updatingIds.delete(id);
        return { updatingIds };
      });
    }
  },

  undoEntries: async (ids) => {
    const deleted = unwrapResult(await commands.undoDictionaryEntries(ids));
    const deletedIds = new Set(deleted.map((entry) => entry.id));
    set((state) => ({
      entries: state.entries.filter((entry) => !deletedIds.has(entry.id)),
      recentlyLearnedEntries: state.recentlyLearnedEntries.filter(
        (entry) => !deletedIds.has(entry.id),
      ),
    }));
  },

  setRecentlyLearnedEntries: (entries) => {
    set((state) => ({
      recentlyLearnedEntries: entries,
      entries: sortEntries([
        ...state.entries.filter(
          (entry) => !entries.some((learned) => learned.id === entry.id),
        ),
        ...entries,
      ]),
    }));
  },

  loadCandidates: async () => {
    const candidates = unwrapResult(await commands.listLearnCandidates());
    set({ candidates: sortCandidates(candidates) });
  },

  approveCandidate: async (phrase, replacementOf) => {
    const entry = unwrapResult(
      await commands.approveLearnCandidate(phrase, replacementOf),
    );
    if (entry) {
      set((state) => ({
        entries: sortEntries([
          ...state.entries.filter((current) => current.id !== entry.id),
          entry,
        ]),
      }));
    }
    await get().loadCandidates();
  },

  rejectCandidate: async (phrase) => {
    unwrapResult(await commands.rejectLearnCandidate(phrase));
    await get().loadCandidates();
  },

  setEntryActive: async (id, active) => {
    set((state) => ({
      updatingIds: new Set([...state.updatingIds, id]),
    }));
    try {
      unwrapResult(await commands.setDictionaryEntryActive(id, active));
      set((state) => ({
        entries: state.entries.map((entry) =>
          entry.id === id ? { ...entry, active, needs_review: false } : entry,
        ),
        recentlyLearnedEntries: state.recentlyLearnedEntries.map((entry) =>
          entry.id === id ? { ...entry, active, needs_review: false } : entry,
        ),
      }));
    } finally {
      set((state) => {
        const updatingIds = new Set(state.updatingIds);
        updatingIds.delete(id);
        return { updatingIds };
      });
    }
  },

  loadDiagnostics: async () => {
    const diagnostics = unwrapResult(await commands.getDictionaryDiagnostics());
    set({ diagnostics });
  },

  resetDiagnostics: async () => {
    unwrapResult(await commands.resetDictionaryDiagnostics());
    await get().loadDiagnostics();
  },
}));
