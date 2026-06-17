import React from "react";
import { Pencil, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { SnippetEntry } from "@/bindings";
import { Button } from "@/components/ui/Button";

interface SnippetEntryRowProps {
  entry: SnippetEntry;
  isUpdating: boolean;
  onEdit: (entry: SnippetEntry) => void;
  onDelete: (entry: SnippetEntry) => void;
}

export const SnippetEntryRow: React.FC<SnippetEntryRowProps> = ({
  entry,
  isUpdating,
  onEdit,
  onDelete,
}) => {
  const { t } = useTranslation();
  const updated = entry.updated_at_ms
    ? new Intl.DateTimeFormat(undefined, {
        dateStyle: "medium",
        timeStyle: "short",
      }).format(new Date(entry.updated_at_ms))
    : "";
  const preview = entry.content.replace(/\s+/g, " ").trim();

  return (
    <div className="grid grid-cols-[1fr_auto] gap-3 items-center px-3 py-3 border-b border-mid-gray/10 last:border-b-0">
      <div className="min-w-0">
        <div className="font-medium break-words" title={entry.trigger}>
          {entry.trigger}
        </div>
        <div
          className="mt-1 text-sm text-mid-gray break-words line-clamp-2"
          title={preview}
        >
          {preview}
        </div>
        {updated && <div className="mt-1 text-xs text-mid-gray">{updated}</div>}
      </div>

      <div className="flex gap-1">
        <Button
          type="button"
          variant="ghost"
          size="sm"
          disabled={isUpdating}
          onClick={() => onEdit(entry)}
          aria-label={t("settings.snippets.editEntry", {
            trigger: entry.trigger,
          })}
          title={t("settings.snippets.editEntry", { trigger: entry.trigger })}
        >
          <Pencil className="h-4 w-4" />
        </Button>
        <Button
          type="button"
          variant="danger-ghost"
          size="sm"
          disabled={isUpdating}
          onClick={() => onDelete(entry)}
          aria-label={t("settings.snippets.deleteEntry", {
            trigger: entry.trigger,
          })}
          title={t("settings.snippets.deleteEntry", {
            trigger: entry.trigger,
          })}
        >
          <Trash2 className="h-4 w-4" />
        </Button>
      </div>
    </div>
  );
};
