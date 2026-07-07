import React from "react";

interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?:
    | "primary"
    | "primary-soft"
    | "secondary"
    | "danger"
    | "danger-ghost"
    | "ghost";
  size?: "sm" | "md" | "lg";
}

export const Button: React.FC<ButtonProps> = ({
  children,
  className = "",
  variant = "primary",
  size = "md",
  ...props
}) => {
  const baseClasses =
    "font-medium rounded-lg border focus-visible:outline-none transition-colors disabled:opacity-50 disabled:cursor-not-allowed cursor-pointer";

  const variantClasses = {
    primary:
      "text-accent-fg bg-accent border-accent hover:bg-accent/85 focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2",
    "primary-soft":
      "text-text bg-accent/15 border-transparent hover:bg-accent/25 focus-visible:ring-2 focus-visible:ring-accent",
    secondary:
      "text-text bg-surface border-border hover:bg-mid-gray/20 hover:border-border-strong focus-visible:ring-2 focus-visible:ring-accent",
    danger:
      "text-white bg-danger border-danger hover:opacity-90 focus-visible:ring-2 focus-visible:ring-danger",
    "danger-ghost":
      "text-danger border-transparent hover:bg-danger-bg focus-visible:ring-2 focus-visible:ring-danger",
    ghost:
      "text-current border-transparent hover:bg-surface focus-visible:ring-2 focus-visible:ring-accent",
  };

  const sizeClasses = {
    sm: "px-2 py-1 text-xs",
    md: "px-4 py-1.5 text-sm",
    lg: "px-4 py-2 text-base",
  };

  return (
    <button
      className={`${baseClasses} ${variantClasses[variant]} ${sizeClasses[size]} ${className}`}
      {...props}
    >
      {children}
    </button>
  );
};
