import { create } from "zustand";
import {
  commands,
  type SnippetEntry,
  type SnippetEntryInput,
  type SnippetEntryUpdate,
} from "@/bindings";

type SnippetsState = {
  entries: SnippetEntry[];
  isLoading: boolean;
  entriesLoaded: boolean;
  updatingIds: Set<string>;
  loadEntries: () => Promise<void>;
  addEntry: (input: SnippetEntryInput) => Promise<SnippetEntry>;
  updateEntry: (
    id: string,
    update: SnippetEntryUpdate,
  ) => Promise<SnippetEntry>;
  deleteEntry: (id: string) => Promise<void>;
};

const sortEntries = (entries: SnippetEntry[]) =>
  [...entries].sort((left, right) => {
    const updatedDiff = (right.updated_at_ms ?? 0) - (left.updated_at_ms ?? 0);
    if (updatedDiff !== 0) {
      return updatedDiff;
    }

    return left.trigger.localeCompare(right.trigger);
  });

const unwrapResult = <T>(
  result: { status: "ok"; data: T } | { status: "error"; error: string },
) => {
  if (result.status === "error") {
    throw new Error(result.error);
  }

  return result.data;
};

export const useSnippetsStore = create<SnippetsState>()((set) => ({
  entries: [],
  isLoading: false,
  entriesLoaded: false,
  updatingIds: new Set<string>(),

  loadEntries: async () => {
    set({ isLoading: true });
    try {
      const entries = unwrapResult(await commands.listSnippetEntries());
      set({ entries: sortEntries(entries), entriesLoaded: true });
    } finally {
      set({ isLoading: false });
    }
  },

  addEntry: async (input) => {
    const entry = unwrapResult(await commands.addSnippetEntry(input));
    set((state) => ({ entries: sortEntries([...state.entries, entry]) }));
    return entry;
  },

  updateEntry: async (id, update) => {
    set((state) => ({
      updatingIds: new Set([...state.updatingIds, id]),
    }));
    try {
      const entry = unwrapResult(await commands.updateSnippetEntry(id, update));
      set((state) => ({
        entries: sortEntries(
          state.entries.map((current) =>
            current.id === entry.id ? entry : current,
          ),
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
      unwrapResult(await commands.deleteSnippetEntry(id));
      set((state) => ({
        entries: state.entries.filter((entry) => entry.id !== id),
      }));
    } finally {
      set((state) => {
        const updatingIds = new Set(state.updatingIds);
        updatingIds.delete(id);
        return { updatingIds };
      });
    }
  },
}));
