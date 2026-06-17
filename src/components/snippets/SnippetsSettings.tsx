import React, { useEffect, useMemo, useState } from "react";
import { toast } from "sonner";
import { useTranslation } from "react-i18next";
import type {
  SnippetEntry,
  SnippetEntryInput,
  SnippetEntryUpdate,
} from "@/bindings";
import { useSnippetsStore } from "@/stores/snippetsStore";
import { SnippetEntryEditor } from "./SnippetEntryEditor";
import { SnippetEntryRow } from "./SnippetEntryRow";
import { SnippetToolbar } from "./SnippetToolbar";

export const SnippetsSettings: React.FC = () => {
  const { t } = useTranslation();
  const {
    entries,
    isLoading,
    updatingIds,
    loadEntries,
    addEntry,
    updateEntry,
    deleteEntry,
  } = useSnippetsStore();
  const [search, setSearch] = useState("");
  const [showEditor, setShowEditor] = useState(false);
  const [editingEntry, setEditingEntry] = useState<SnippetEntry | null>(null);

  useEffect(() => {
    loadEntries().catch(() => {
      toast.error(t("settings.snippets.errors.load"));
    });
  }, [loadEntries, t]);

  const filteredEntries = useMemo(() => {
    const query = search.trim().toLowerCase();
    if (!query) return entries;

    return entries.filter((entry) =>
      [entry.trigger, entry.content].some((value) =>
        value.toLowerCase().includes(query),
      ),
    );
  }, [entries, search]);

  const multiLineCount = entries.filter((entry) =>
    entry.content.includes("\n"),
  ).length;

  const handleSave = async (input: SnippetEntryInput) => {
    try {
      if (editingEntry) {
        const update: SnippetEntryUpdate = {
          trigger: input.trigger,
          content: input.content,
        };
        await updateEntry(editingEntry.id, update);
      } else {
        await addEntry(input);
      }
      setShowEditor(false);
      setEditingEntry(null);
    } catch (error) {
      toast.error(
        t(
          editingEntry
            ? "settings.snippets.errors.update"
            : "settings.snippets.errors.add",
        ),
        { description: error instanceof Error ? error.message : undefined },
      );
    }
  };

  const handleDelete = async (entry: SnippetEntry) => {
    try {
      await deleteEntry(entry.id);
    } catch (error) {
      toast.error(t("settings.snippets.errors.delete"), {
        description: error instanceof Error ? error.message : undefined,
      });
    }
  };

  return (
    <div className="max-w-3xl w-full mx-auto space-y-5">
      <div className="space-y-1">
        <h1 className="text-xl font-semibold">
          {t("settings.snippets.title")}
        </h1>
        <p className="text-sm text-mid-gray">
          {t("settings.snippets.description")}
        </p>
      </div>

      <SnippetToolbar
        search={search}
        onSearchChange={setSearch}
        onAdd={() => {
          setEditingEntry(null);
          setShowEditor(true);
        }}
      />

      <div className="grid grid-cols-2 gap-2 text-sm">
        <div className="border border-mid-gray/20 rounded-lg px-3 py-2">
          {t("settings.snippets.counts.total", { count: entries.length })}
        </div>
        <div className="border border-mid-gray/20 rounded-lg px-3 py-2">
          {t("settings.snippets.counts.multiline", {
            count: multiLineCount,
          })}
        </div>
      </div>

      {showEditor && (
        <SnippetEntryEditor
          entry={editingEntry}
          onCancel={() => {
            setShowEditor(false);
            setEditingEntry(null);
          }}
          onSave={handleSave}
        />
      )}

      <div
        data-testid="snippet-entries-list"
        className="border border-mid-gray/20 rounded-lg overflow-hidden"
      >
        {isLoading ? (
          <div className="px-3 py-4 text-sm text-mid-gray">
            {t("common.loading")}
          </div>
        ) : filteredEntries.length === 0 ? (
          <div className="px-3 py-4 text-sm text-mid-gray">
            {entries.length === 0
              ? t("settings.snippets.empty")
              : t("settings.snippets.noResults")}
          </div>
        ) : (
          filteredEntries.map((entry) => (
            <SnippetEntryRow
              key={entry.id}
              entry={entry}
              isUpdating={updatingIds.has(entry.id)}
              onEdit={(nextEntry) => {
                setEditingEntry(nextEntry);
                setShowEditor(true);
              }}
              onDelete={handleDelete}
            />
          ))
        )}
      </div>
    </div>
  );
};
