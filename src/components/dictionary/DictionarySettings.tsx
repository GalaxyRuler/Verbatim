import React, { useEffect, useMemo, useState } from "react";
import { Undo2 } from "lucide-react";
import { toast } from "sonner";
import { useTranslation } from "react-i18next";
import type {
  DictionaryEntry,
  DictionaryEntryInput,
  DictionaryEntryUpdate,
} from "@/bindings";
import { useDictionaryStore } from "@/stores/dictionaryStore";
import { useSettings } from "@/hooks/useSettings";
import { Button } from "@/components/ui/Button";
import { ToggleSwitch } from "@/components/ui/ToggleSwitch";
import { DictionaryEntryEditor } from "./DictionaryEntryEditor";
import { DictionaryEntryRow } from "./DictionaryEntryRow";
import { DictionaryToolbar } from "./DictionaryToolbar";

export const DictionarySettings: React.FC = () => {
  const { t } = useTranslation();
  const {
    entries,
    recentlyLearnedEntries,
    isLoading,
    updatingIds,
    loadEntries,
    addEntry,
    updateEntry,
    deleteEntry,
    undoEntries,
  } = useDictionaryStore();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const [search, setSearch] = useState("");
  const [showEditor, setShowEditor] = useState(false);
  const [editingEntry, setEditingEntry] = useState<DictionaryEntry | null>(
    null,
  );
  const autoAddDictionaryWords =
    getSetting("auto_add_dictionary_words") || false;

  useEffect(() => {
    loadEntries().catch(() => {
      toast.error(t("settings.dictionary.errors.load"));
    });
  }, [loadEntries, t]);

  const filteredEntries = useMemo(() => {
    const query = search.trim().toLowerCase();
    if (!query) return entries;

    return entries.filter((entry) =>
      [entry.phrase, entry.replacement_of ?? ""].some((value) =>
        value.toLowerCase().includes(query),
      ),
    );
  }, [entries, search]);

  const autoLearnedCount = entries.filter(
    (entry) => entry.source === "auto_learned",
  ).length;
  const starredCount = entries.filter(
    (entry) => entry.priority === "starred",
  ).length;

  const handleSave = async (input: DictionaryEntryInput) => {
    try {
      if (editingEntry) {
        const update: DictionaryEntryUpdate = {
          phrase: input.phrase,
          replacement_of: input.replacement_of ?? null,
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
            ? "settings.dictionary.errors.update"
            : "settings.dictionary.errors.add",
        ),
        { description: error instanceof Error ? error.message : undefined },
      );
    }
  };

  const handleDelete = async (entry: DictionaryEntry) => {
    try {
      await deleteEntry(entry.id);
    } catch (error) {
      toast.error(t("settings.dictionary.errors.delete"), {
        description: error instanceof Error ? error.message : undefined,
      });
    }
  };

  const handleToggleStar = async (entry: DictionaryEntry) => {
    const priority = entry.priority === "starred" ? "normal" : "starred";
    try {
      await updateEntry(entry.id, { priority });
    } catch (error) {
      toast.error(t("settings.dictionary.errors.update"), {
        description: error instanceof Error ? error.message : undefined,
      });
    }
  };

  const handleUndo = async () => {
    try {
      await undoEntries(recentlyLearnedEntries.map((entry) => entry.id));
    } catch (error) {
      toast.error(t("settings.dictionary.errors.delete"), {
        description: error instanceof Error ? error.message : undefined,
      });
    }
  };

  return (
    <div className="max-w-3xl w-full mx-auto space-y-5">
      <div className="space-y-1">
        <h1 className="text-xl font-semibold">
          {t("settings.dictionary.title")}
        </h1>
        <p className="text-sm text-mid-gray">
          {t("settings.dictionary.description")}
        </p>
      </div>

      <DictionaryToolbar
        search={search}
        onSearchChange={setSearch}
        onAdd={() => {
          setEditingEntry(null);
          setShowEditor(true);
        }}
      />

      <div className="grid grid-cols-3 gap-2 text-sm">
        <div className="border border-mid-gray/20 rounded-lg px-3 py-2">
          {t("settings.dictionary.counts.total", { count: entries.length })}
        </div>
        <div className="border border-mid-gray/20 rounded-lg px-3 py-2">
          {t("settings.dictionary.counts.autoLearned", {
            count: autoLearnedCount,
          })}
        </div>
        <div className="border border-mid-gray/20 rounded-lg px-3 py-2">
          {t("settings.dictionary.counts.starred", { count: starredCount })}
        </div>
      </div>

      <ToggleSwitch
        checked={autoAddDictionaryWords}
        onChange={(enabled) =>
          updateSetting("auto_add_dictionary_words", enabled)
        }
        isUpdating={isUpdating("auto_add_dictionary_words")}
        label={t("settings.dictionary.autoAdd.label")}
        description={t("settings.dictionary.autoAdd.description")}
        descriptionMode="tooltip"
        grouped={false}
      />

      {recentlyLearnedEntries.length > 0 && (
        <div
          role="status"
          aria-live="polite"
          data-testid="dictionary-recently-learned"
          className="border border-logo-primary/30 rounded-lg bg-logo-primary/10 px-3 py-2 text-sm flex flex-col sm:flex-row sm:items-center gap-2 justify-between"
        >
          <div>
            <div className="font-medium text-logo-primary">
              {t("settings.dictionary.recentlyLearned.title")}
            </div>
            <div>
              {t("settings.dictionary.recentlyLearned.description", {
                phrases: recentlyLearnedEntries
                  .map((entry) => entry.phrase)
                  .join(", "),
              })}
            </div>
          </div>
          <Button
            type="button"
            variant="secondary"
            size="sm"
            onClick={handleUndo}
            className="inline-flex items-center gap-2 self-start sm:self-auto"
          >
            <Undo2 className="h-4 w-4" />
            <span>{t("settings.dictionary.recentlyLearned.undo")}</span>
          </Button>
        </div>
      )}

      {showEditor && (
        <DictionaryEntryEditor
          entry={editingEntry}
          onCancel={() => {
            setShowEditor(false);
            setEditingEntry(null);
          }}
          onSave={handleSave}
        />
      )}

      <div
        data-testid="dictionary-entries-list"
        className="border border-mid-gray/20 rounded-lg overflow-hidden"
      >
        {isLoading ? (
          <div className="px-3 py-4 text-sm text-mid-gray">
            {t("common.loading")}
          </div>
        ) : filteredEntries.length === 0 ? (
          <div className="px-3 py-4 text-sm text-mid-gray">
            {entries.length === 0
              ? t("settings.dictionary.empty")
              : t("settings.dictionary.noResults")}
          </div>
        ) : (
          filteredEntries.map((entry) => (
            <DictionaryEntryRow
              key={entry.id}
              entry={entry}
              isUpdating={updatingIds.has(entry.id)}
              onEdit={(nextEntry) => {
                setEditingEntry(nextEntry);
                setShowEditor(true);
              }}
              onDelete={handleDelete}
              onToggleStar={handleToggleStar}
            />
          ))
        )}
      </div>
    </div>
  );
};
