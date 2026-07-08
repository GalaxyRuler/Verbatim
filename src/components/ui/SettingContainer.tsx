import React, { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Info } from "lucide-react";
import { Tooltip } from "./Tooltip";

interface SettingContainerProps {
  title: string;
  description: string;
  children: React.ReactNode;
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  layout?: "horizontal" | "stacked";
  disabled?: boolean;
  tooltipPosition?: "top" | "bottom";
}

export const SettingContainer: React.FC<SettingContainerProps> = ({
  title,
  description,
  children,
  descriptionMode = "tooltip",
  grouped = false,
  layout = "horizontal",
  disabled = false,
  tooltipPosition = "top",
}) => {
  const { t } = useTranslation();
  const [showTooltip, setShowTooltip] = useState(false);
  const tooltipRef = useRef<HTMLDivElement>(null);

  // Handle click outside to close tooltip
  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (
        tooltipRef.current &&
        !tooltipRef.current.contains(event.target as Node)
      ) {
        setShowTooltip(false);
      }
    };

    if (showTooltip) {
      document.addEventListener("mousedown", handleClickOutside);
      return () =>
        document.removeEventListener("mousedown", handleClickOutside);
    }
  }, [showTooltip]);

  const toggleTooltip = () => {
    setShowTooltip(!showTooltip);
  };

  const effectiveTooltipPosition =
    layout === "stacked" ? "top" : tooltipPosition;

  const infoTrigger = (
    <div
      ref={tooltipRef}
      className="relative"
      onMouseEnter={() => setShowTooltip(true)}
      onMouseLeave={() => setShowTooltip(false)}
    >
      <button
        type="button"
        data-testid="setting-info-trigger"
        aria-label={t("common.moreInfo")}
        aria-expanded={showTooltip}
        onClick={toggleTooltip}
        onBlur={() => setShowTooltip(false)}
        className="flex items-center text-mid-gray hover:text-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent rounded-md transition-colors duration-200"
      >
        <Info size={16} aria-hidden />
      </button>
      {showTooltip && (
        <Tooltip targetRef={tooltipRef} position={effectiveTooltipPosition}>
          <p className="text-sm text-center leading-relaxed">{description}</p>
        </Tooltip>
      )}
    </div>
  );

  const containerClasses = grouped
    ? "px-4 p-2"
    : "px-4 p-2 rounded-lg border border-mid-gray/20";
  const disabledTextClass = disabled ? "text-mid-gray" : "";

  if (layout === "stacked") {
    if (descriptionMode === "tooltip") {
      return (
        <div className={containerClasses}>
          <div className="flex items-center gap-2 mb-2">
            <h3 className={`text-sm font-medium ${disabledTextClass}`}>
              {title}
            </h3>
            {infoTrigger}
          </div>
          <div className="w-full">{children}</div>
        </div>
      );
    }

    return (
      <div className={containerClasses}>
        <div className="mb-2">
          <h3 className={`text-sm font-medium ${disabledTextClass}`}>
            {title}
          </h3>
          <p className={`text-sm ${disabledTextClass}`}>{description}</p>
        </div>
        <div className="w-full">{children}</div>
      </div>
    );
  }

  // Horizontal layout (default)
  const horizontalContainerClasses = grouped
    ? "flex items-center justify-between px-4 p-2"
    : "flex items-center justify-between px-4 p-2 rounded-lg border border-mid-gray/20";

  if (descriptionMode === "tooltip") {
    return (
      <div className={horizontalContainerClasses}>
        <div className="max-w-2/3">
          <div className="flex items-center gap-2">
            <h3 className={`text-sm font-medium ${disabledTextClass}`}>
              {title}
            </h3>
            {infoTrigger}
          </div>
        </div>
        <div className="relative">{children}</div>
      </div>
    );
  }

  return (
    <div className={horizontalContainerClasses}>
      <div className="max-w-2/3">
        <h3 className={`text-sm font-medium ${disabledTextClass}`}>{title}</h3>
        <p className={`text-sm ${disabledTextClass}`}>{description}</p>
      </div>
      <div className="relative">{children}</div>
    </div>
  );
};
