import React from "react";

/* eslint-disable i18next/no-literal-string */

const VerbatimTextLogo = ({
  width,
  height,
  className,
}: {
  width?: number;
  height?: number;
  className?: string;
}) => {
  return (
    <svg
      width={width}
      height={height}
      className={className}
      viewBox="0 0 640 128"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      role="img"
      aria-label="Verbatim"
    >
      <g fill="var(--color-logo-primary)">
        <rect x="20" y="54" width="8" height="20" rx="4" />
        <rect x="36" y="41" width="8" height="46" rx="4" />
        <rect x="52" y="30" width="8" height="68" rx="4" />
        <rect x="68" y="41" width="8" height="46" rx="4" />
        <rect x="84" y="50" width="8" height="28" rx="4" />
        <rect x="100" y="55" width="8" height="18" rx="4" />
        <rect x="116" y="59" width="8" height="10" rx="4" />
        <circle cx="138" cy="64" r="4" />
        <circle cx="156" cy="64" r="4" />
        <circle cx="174" cy="64" r="4" />
        <circle cx="192" cy="64" r="4" />
        <circle cx="210" cy="64" r="4" />
        <rect x="244" y="32" width="7" height="64" rx="3.5" />
      </g>
      <text
        x="284"
        y="86"
        fill="var(--color-logo-stroke)"
        fontFamily="Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, Segoe UI, sans-serif"
        fontSize="68"
        fontWeight="650"
        letterSpacing="0"
      >
        Verbatim
      </text>
    </svg>
  );
};

export default VerbatimTextLogo;
