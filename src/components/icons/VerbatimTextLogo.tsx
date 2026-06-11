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
      viewBox="0 0 520 96"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      role="img"
      aria-label="Verbatim"
    >
      <path
        d="M24 70H136"
        stroke="var(--color-logo-primary)"
        strokeWidth="8"
        strokeLinecap="round"
      />
      <path
        d="M24 26H136"
        stroke="var(--color-logo-stroke)"
        strokeWidth="8"
        strokeLinecap="round"
      />
      <path
        d="M58 26L82 70L106 26"
        stroke="var(--color-logo-primary)"
        strokeWidth="10"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <text
        x="160"
        y="68"
        fill="var(--color-logo-stroke)"
        fontFamily="Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, Segoe UI, sans-serif"
        fontSize="54"
        fontWeight="800"
        letterSpacing="0"
      >
        Verbatim
      </text>
    </svg>
  );
};

export default VerbatimTextLogo;
