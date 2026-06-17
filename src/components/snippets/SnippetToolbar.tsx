import React from "react";
import { Plus, Search } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/Button";
import { Input } from "@/components/ui/Input";

interface SnippetToolbarProps {
  search: string;
  onSearchChange: (value: string) => void;
  onAdd: () => void;
}

export const SnippetToolbar: React.FC<SnippetToolbarProps> = ({
  search,
  onSearchChange,
  onAdd,
}) => {
  const { t } = useTranslation();

  return (
    <div className="flex flex-col sm:flex-row gap-2 sm:items-center">
      <label className="relative flex-1">
        <span className="sr-only">{t("settings.snippets.search")}</span>
        <Search className="absolute start-3 top-1/2 h-4 w-4 -translate-y-1/2 text-mid-gray" />
        <Input
          value={search}
          onChange={(event) => onSearchChange(event.target.value)}
          placeholder={t("settings.snippets.search")}
          className="w-full ps-9"
        />
      </label>
      <Button
        type="button"
        variant="primary"
        size="md"
        onClick={onAdd}
        className="inline-flex items-center justify-center gap-2"
      >
        <Plus className="h-4 w-4" />
        <span>{t("settings.snippets.add")}</span>
      </Button>
    </div>
  );
};
