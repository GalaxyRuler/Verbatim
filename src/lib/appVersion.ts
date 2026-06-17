import { getVersion } from "@tauri-apps/api/app";

const devVersion = import.meta.env.VITE_VERBATIM_DEV_VERSION?.trim();

export const getDisplayVersion = async (): Promise<string> => {
  if (devVersion) {
    return devVersion;
  }

  return getVersion();
};
