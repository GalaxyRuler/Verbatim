import React, { useState } from "react";
import { Check, Copy } from "lucide-react";
import { SettingContainer } from "./SettingContainer";

interface TextDisplayProps {
  label: string;
  description: string;
  value: string;
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  placeholder?: string;
  copyable?: boolean;
  monospace?: boolean;
  onCopy?: (value: string) => void;
}

export const TextDisplay: React.FC<TextDisplayProps> = ({
  label,
  description,
  value,
  descriptionMode = "tooltip",
  grouped = false,
  placeholder = "Not available",
  copyable = false,
  monospace = false,
  onCopy,
}) => {
  const [showCopied, setShowCopied] = useState(false);

  const handleCopy = async () => {
    if (!value || !copyable) return;

    try {
      await navigator.clipboard.writeText(value);
      setShowCopied(true);
      setTimeout(() => setShowCopied(false), 1500);
      if (onCopy) {
        onCopy(value);
      }
    } catch (err) {
      console.error("Failed to copy to clipboard:", err);
    }
  };

  const displayValue = value || placeholder;
  const textClasses = monospace ? "font-mono break-all" : "break-words";

  return (
    <SettingContainer
      title={label}
      description={description}
      descriptionMode={descriptionMode}
      grouped={grouped}
      layout="stacked"
    >
      <div className="flex items-center space-x-2">
        <div className="flex-1 min-w-0">
          <div
            className={`px-2 min-h-8 flex items-center bg-mid-gray/10 border border-border rounded-md text-xs ${textClasses} ${!value ? "opacity-60" : ""}`}
          >
            {displayValue}
          </div>
        </div>
        {copyable && value && (
          <button
            onClick={handleCopy}
            className="flex items-center justify-center px-2 py-1 w-12 min-h-8 text-xs font-semibold bg-mid-gray/10 border border-border hover:border-border-strong hover:text-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent rounded-md transition-all duration-150 flex-shrink-0 cursor-pointer"
            title="Copy to clipboard"
            aria-label="Copy to clipboard"
          >
            {showCopied ? (
              <Check size={16} aria-hidden />
            ) : (
              <Copy size={16} aria-hidden />
            )}
          </button>
        )}
      </div>
    </SettingContainer>
  );
};
