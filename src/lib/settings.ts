import type { AppSettings as GeneratedAppSettings } from "@/bindings";

/**
 * Settings returned by the backend after it has reconciled legacy persisted
 * values with current defaults. The wire type remains tolerant so old stores
 * can be decoded; command consumers always receive this complete shape.
 */
export type AppSettings = Required<GeneratedAppSettings>;

export const asAppSettings = (settings: GeneratedAppSettings): AppSettings =>
  settings as AppSettings;
