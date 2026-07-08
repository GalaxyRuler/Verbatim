import React from "react";
import { ChevronDown } from "lucide-react";

type ModelStatus =
  | "ready"
  | "loading"
  | "downloading"
  | "verifying"
  | "extracting"
  | "error"
  | "unloaded"
  | "none";

interface ModelStatusButtonProps {
  status: ModelStatus;
  displayText: string;
  isDropdownOpen: boolean;
  onClick: () => void;
  className?: string;
}

const ModelStatusButton: React.FC<ModelStatusButtonProps> = ({
  status,
  displayText,
  isDropdownOpen,
  onClick,
  className = "",
}) => {
  const getStatusColor = (status: ModelStatus): string => {
    switch (status) {
      case "ready":
        return "bg-success";
      case "loading":
        return "bg-warning animate-pulse";
      case "downloading":
        return "bg-accent animate-pulse";
      case "verifying":
        return "bg-warning animate-pulse";
      case "extracting":
        return "bg-warning animate-pulse";
      case "error":
        return "bg-danger";
      case "unloaded":
        return "bg-text-disabled";
      case "none":
        return "bg-danger";
      default:
        return "bg-text-disabled";
    }
  };

  return (
    <button
      onClick={onClick}
      className={`flex items-center gap-2 hover:text-text/80 transition-colors ${className}`}
      title={`Model status: ${displayText}`}
    >
      <div className={`w-2 h-2 rounded-full ${getStatusColor(status)}`} />
      <span className="max-w-28 truncate">{displayText}</span>
      <ChevronDown
        size={16}
        aria-hidden
        className={`shrink-0 transition-transform ${isDropdownOpen ? "rotate-180" : ""}`}
      />
    </button>
  );
};

export default ModelStatusButton;
