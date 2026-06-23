import type {
  AdaptiveSettingsDomain,
  AudioSettingsDomain,
  DiagnosticsSettingsDomain,
  GeneralSettingsDomain,
  InsertionSettingsDomain,
  ModelsSettingsDomain,
  PostProcessingSettingsDomain,
  PrivacySettingsDomain,
  SettingsStoreDocument,
  ShortcutsSettingsDomain,
} from "@/bindings";

type DomainFields<T extends { version: number }> = Omit<T, "version">;

type SettingsDomainVersions = {
  general: number;
  audio: number;
  insertion: number;
  privacy: number;
  models: number;
  post_processing: number;
  diagnostics: number;
  adaptive: number;
  shortcuts: number;
};

export type AppSettings = {
  settings_schema_version: number;
  settings_domain_versions: SettingsDomainVersions;
} & DomainFields<GeneralSettingsDomain> &
  DomainFields<AudioSettingsDomain> &
  DomainFields<InsertionSettingsDomain> &
  DomainFields<PrivacySettingsDomain> &
  DomainFields<ModelsSettingsDomain> &
  DomainFields<PostProcessingSettingsDomain> &
  DomainFields<DiagnosticsSettingsDomain> &
  DomainFields<AdaptiveSettingsDomain> &
  DomainFields<ShortcutsSettingsDomain>;

const stripDomainVersion = <T extends { version: number }>(
  domain: T,
): Omit<T, "version"> => {
  const { version: _version, ...settings } = domain;
  return settings;
};

export const settingsFromDocument = (
  document: SettingsStoreDocument,
): AppSettings => {
  const { domains } = document;

  return {
    settings_schema_version: document.settings_schema_version,
    settings_domain_versions: {
      general: domains.general.version,
      audio: domains.audio.version,
      insertion: domains.insertion.version,
      privacy: domains.privacy.version,
      models: domains.models.version,
      post_processing: domains.post_processing.version,
      diagnostics: domains.diagnostics.version,
      adaptive: domains.adaptive.version,
      shortcuts: domains.shortcuts.version,
    },
    ...stripDomainVersion(domains.general),
    ...stripDomainVersion(domains.audio),
    ...stripDomainVersion(domains.insertion),
    ...stripDomainVersion(domains.privacy),
    ...stripDomainVersion(domains.models),
    ...stripDomainVersion(domains.post_processing),
    ...stripDomainVersion(domains.diagnostics),
    ...stripDomainVersion(domains.adaptive),
    ...stripDomainVersion(domains.shortcuts),
  };
};
