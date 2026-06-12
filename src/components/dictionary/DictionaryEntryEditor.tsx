import React, { useEffect, useState } from "react";
import { Check, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { DictionaryEntry, DictionaryEntryInput } from "@/bindings";
import { Button } from "@/components/ui/Button";
import { Input } from "@/components/ui/Input";

interface DictionaryEntryEditorProps {
  entry?: DictionaryEntry | null;
  onCancel: () => void;
  onSave: (input: DictionaryEntryInput) => Promise<void>;
}

export const DictionaryEntryEditor: React.FC<DictionaryEntryEditorProps> = ({
  entry,
  onCancel,
  onSave,
}) => {
  const { t } = useTranslation();
  const [phrase, setPhrase] = useState(entry?.phrase ?? "");
  const [replacementOf, setReplacementOf] = useState(
    entry?.replacement_of ?? "",
  );
  const [isSaving, setIsSaving] = useState(false);

  useEffect(() => {
    setPhrase(entry?.phrase ?? "");
    setReplacementOf(entry?.replacement_of ?? "");
  }, [entry]);

  const canSave = phrase.trim().length > 0 && phrase.trim().length <= 120;

  const handleSave = async () => {
    if (!canSave) return;
    setIsSaving(true);
    try {
      await onSave({
        phrase: phrase.trim(),
        replacement_of: replacementOf.trim() || null,
      });
      if (!entry) {
        setPhrase("");
        setReplacementOf("");
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
      <div className="grid grid-cols-1 md:grid-cols-[1fr_1fr_auto] gap-2 items-end">
        <label className="space-y-1">
          <span className="block text-xs font-medium text-mid-gray">
            {t("settings.dictionary.phrase")}
          </span>
          <Input
            value={phrase}
            onChange={(event) => setPhrase(event.target.value)}
            maxLength={120}
            disabled={isSaving}
            className="w-full"
          />
        </label>
        <label className="space-y-1">
          <span className="block text-xs font-medium text-mid-gray">
            {t("settings.dictionary.replacementOf")}
          </span>
          <Input
            value={replacementOf}
            onChange={(event) => setReplacementOf(event.target.value)}
            maxLength={120}
            disabled={isSaving}
            className="w-full"
            placeholder={t("settings.dictionary.replacementPlaceholder")}
          />
        </label>
        <div className="flex gap-2 justify-end">
          <Button
            type="submit"
            variant="primary"
            size="sm"
            disabled={!canSave || isSaving}
            aria-label={t("settings.dictionary.save")}
            title={t("settings.dictionary.save")}
          >
            <Check className="h-4 w-4" />
          </Button>
          <Button
            type="button"
            variant="secondary"
            size="sm"
            onClick={onCancel}
            disabled={isSaving}
            aria-label={t("settings.dictionary.cancel")}
            title={t("settings.dictionary.cancel")}
          >
            <X className="h-4 w-4" />
          </Button>
        </div>
      </div>
    </form>
  );
};
