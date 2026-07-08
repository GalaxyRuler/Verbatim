import React from "react";
import { Pencil, Star, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { DictionaryEntry } from "@/bindings";
import { Button } from "@/components/ui/Button";
import Badge from "@/components/ui/Badge";

interface DictionaryEntryRowProps {
  entry: DictionaryEntry;
  isUpdating: boolean;
  onEdit: (entry: DictionaryEntry) => void;
  onDelete: (entry: DictionaryEntry) => void;
  onToggleStar: (entry: DictionaryEntry) => void;
}

export const DictionaryEntryRow: React.FC<DictionaryEntryRowProps> = ({
  entry,
  isUpdating,
  onEdit,
  onDelete,
  onToggleStar,
}) => {
  const { t } = useTranslation();
  const isStarred = entry.priority === "starred";
  const source = entry.source ?? "manual";
  const isQuarantined = entry.active === false;
  const updated = entry.updated_at_ms
    ? new Intl.DateTimeFormat(undefined, {
        dateStyle: "medium",
        timeStyle: "short",
      }).format(new Date(entry.updated_at_ms))
    : "";

  return (
    <div
      className={`grid grid-cols-[auto_1fr_auto] gap-3 items-center px-3 py-2 border-b border-mid-gray/10 last:border-b-0 ${isQuarantined ? "opacity-60" : ""}`}
    >
      <Button
        type="button"
        variant="ghost"
        size="sm"
        disabled={isUpdating}
        onClick={() => onToggleStar(entry)}
        aria-label={t(
          isStarred
            ? "settings.dictionary.unstarEntry"
            : "settings.dictionary.starEntry",
          { phrase: entry.phrase },
        )}
        title={t(
          isStarred
            ? "settings.dictionary.unstarEntry"
            : "settings.dictionary.starEntry",
          { phrase: entry.phrase },
        )}
        className={isStarred ? "text-accent" : ""}
      >
        <Star className={`h-4 w-4 ${isStarred ? "fill-current" : ""}`} />
      </Button>

      <div className="min-w-0">
        <div className="flex flex-wrap items-center gap-2">
          <span className="font-medium break-words" title={entry.phrase}>
            {entry.phrase}
          </span>
          <Badge variant={source === "auto_learned" ? "success" : "secondary"}>
            {t(`settings.dictionary.source.${source}`)}
          </Badge>
          {isQuarantined && (
            <Badge variant="secondary">
              {t("settings.dictionary.needsReview.quarantined")}
            </Badge>
          )}
        </div>
        <div className="mt-1 flex flex-wrap gap-x-3 gap-y-1 text-xs text-mid-gray">
          {entry.replacement_of && (
            <span>
              {t("settings.dictionary.corrects", {
                replacement: entry.replacement_of,
              })}
            </span>
          )}
          {updated && <span>{updated}</span>}
        </div>
      </div>

      <div className="flex gap-1">
        <Button
          type="button"
          variant="ghost"
          size="sm"
          disabled={isUpdating}
          onClick={() => onEdit(entry)}
          aria-label={t("settings.dictionary.editEntry", {
            phrase: entry.phrase,
          })}
          title={t("settings.dictionary.editEntry", { phrase: entry.phrase })}
        >
          <Pencil className="h-4 w-4" />
        </Button>
        <Button
          type="button"
          variant="danger-ghost"
          size="sm"
          disabled={isUpdating}
          onClick={() => onDelete(entry)}
          aria-label={t("settings.dictionary.deleteEntry", {
            phrase: entry.phrase,
          })}
          title={t("settings.dictionary.deleteEntry", {
            phrase: entry.phrase,
          })}
        >
          <Trash2 className="h-4 w-4" />
        </Button>
      </div>
    </div>
  );
};
