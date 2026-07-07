import React from "react";

interface InputProps extends React.InputHTMLAttributes<HTMLInputElement> {
  variant?: "default" | "compact";
}

export const Input: React.FC<InputProps> = ({
  className = "",
  variant = "default",
  disabled,
  ...props
}) => {
  const baseClasses =
    "text-sm font-semibold bg-mid-gray/10 border border-border rounded-md text-start transition-all duration-150";

  const interactiveClasses = disabled
    ? "opacity-60 cursor-not-allowed"
    : "hover:border-border-strong focus-visible:outline-none focus-visible:border-accent focus-visible:ring-2 focus-visible:ring-accent";

  const variantClasses = {
    default: "px-3 py-2",
    compact: "px-2 py-1",
  } as const;

  return (
    <input
      className={`${baseClasses} ${variantClasses[variant]} ${interactiveClasses} ${className}`}
      disabled={disabled}
      {...props}
    />
  );
};
