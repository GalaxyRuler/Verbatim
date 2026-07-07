import React from "react";
import { AlertCircle, AlertTriangle, Info, CheckCircle } from "lucide-react";

type AlertVariant = "error" | "warning" | "info" | "success";

interface AlertProps {
  variant?: AlertVariant;
  /** When true, removes rounded corners for use inside containers */
  contained?: boolean;
  children: React.ReactNode;
  className?: string;
}

const variantStyles: Record<
  AlertVariant,
  { container: string; icon: string; text: string }
> = {
  error: {
    container: "bg-danger-bg",
    icon: "text-danger",
    text: "text-danger",
  },
  warning: {
    container: "bg-warning-bg",
    icon: "text-warning",
    text: "text-warning",
  },
  info: {
    container: "bg-info-bg",
    icon: "text-info",
    text: "text-info",
  },
  success: {
    container: "bg-success-bg",
    icon: "text-success",
    text: "text-success",
  },
};

const variantIcons: Record<AlertVariant, React.ElementType> = {
  error: AlertCircle,
  warning: AlertTriangle,
  info: Info,
  success: CheckCircle,
};

export const Alert: React.FC<AlertProps> = ({
  variant = "error",
  contained = false,
  children,
  className = "",
}) => {
  const styles = variantStyles[variant];
  const Icon = variantIcons[variant];

  return (
    <div
      className={`flex items-start gap-3 p-4 ${styles.container} ${contained ? "" : "rounded-lg"} ${className}`}
    >
      <Icon className={`w-5 h-5 shrink-0 mt-0.5 ${styles.icon}`} />
      <p className={`text-sm ${styles.text}`}>{children}</p>
    </div>
  );
};
