import React from "react";
import { useTranslation } from "react-i18next";
import ResetIcon from "../icons/ResetIcon";

interface ResetButtonProps {
  onClick: () => void;
  disabled?: boolean;
  className?: string;
  ariaLabel?: string;
  children?: React.ReactNode;
}

export const ResetButton: React.FC<ResetButtonProps> = React.memo(
  ({ onClick, disabled = false, className = "", ariaLabel, children }) => {
    const { t } = useTranslation();

    return (
      <button
        type="button"
        aria-label={ariaLabel ?? t("common.reset")}
        className={`p-1 rounded-md border border-transparent transition-all duration-150 ${
          disabled
            ? "opacity-50 cursor-not-allowed text-text/40"
            : "hover:bg-accent/30 active:bg-accent/50 hover:cursor-pointer hover:border-accent text-text/80"
        } ${className}`}
        onClick={onClick}
        disabled={disabled}
      >
        {children ?? <ResetIcon />}
      </button>
    );
  },
);
