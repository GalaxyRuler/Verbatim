import React, { useEffect, useState } from "react";
import { Check, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { SnippetEntry, SnippetEntryInput } from "@/bindings";
import { Button } from "@/components/ui/Button";
import { Input } from "@/components/ui/Input";
import { Textarea } from "@/components/ui/Textarea";

interface SnippetEntryEditorProps {
  entry?: SnippetEntry | null;
  onCancel: () => void;
  onSave: (input: SnippetEntryInput) => Promise<void>;
}

export const SnippetEntryEditor: React.FC<SnippetEntryEditorProps> = ({
  entry,
  onCancel,
  onSave,
}) => {
  const { t } = useTranslation();
  const [trigger, setTrigger] = useState(entry?.trigger ?? "");
  const [content, setContent] = useState(entry?.content ?? "");
  const [isSaving, setIsSaving] = useState(false);

  useEffect(() => {
    setTrigger(entry?.trigger ?? "");
    setContent(entry?.content ?? "");
  }, [entry]);

  const canSave =
    trigger.trim().length > 0 &&
    trigger.trim().length <= 120 &&
    content.trim().length > 0 &&
    content.trim().length <= 12000;

  const handleSave = async () => {
    if (!canSave) return;
    setIsSaving(true);
    try {
      await onSave({
        trigger: trigger.trim(),
        content: content.trim(),
      });
      if (!entry) {
        setTrigger("");
        setContent("");
      }
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <form
      className="w-full border border-mid-gray/20 rounded-lg p-3 space-y-3"
      onSubmit={(event) => {
        event.preventDefault();
        void handleSave();
      }}
    >
      <div className="grid grid-cols-1 gap-3">
        <label className="space-y-1">
          <span className="block text-xs font-medium text-mid-gray">
            {t("settings.snippets.trigger")}
          </span>
          <Input
            value={trigger}
            onChange={(event) => setTrigger(event.target.value)}
            maxLength={120}
            disabled={isSaving}
            className="w-full"
          />
        </label>
        <label className="space-y-1">
          <span className="block text-xs font-medium text-mid-gray">
            {t("settings.snippets.content")}
          </span>
          <Textarea
            value={content}
            onChange={(event) => setContent(event.target.value)}
            maxLength={12000}
            disabled={isSaving}
            className="w-full"
          />
        </label>
        <div className="flex gap-2 justify-end">
          <Button
            type="submit"
            variant="primary"
            size="sm"
            disabled={!canSave || isSaving}
            aria-label={t("settings.snippets.save")}
            title={t("settings.snippets.save")}
          >
            <Check className="h-4 w-4" />
          </Button>
          <Button
            type="button"
            variant="secondary"
            size="sm"
            onClick={onCancel}
            disabled={isSaving}
            aria-label={t("settings.snippets.cancel")}
            title={t("settings.snippets.cancel")}
          >
            <X className="h-4 w-4" />
          </Button>
        </div>
      </div>
    </form>
  );
};
