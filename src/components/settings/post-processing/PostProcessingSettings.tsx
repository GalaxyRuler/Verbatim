import React, { useCallback, useEffect, useMemo, useState } from "react";
import { Trans, useTranslation } from "react-i18next";
import { Check, Download, Loader2, RefreshCcw, Trash2 } from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import {
  commands,
  type CredentialStoreStatus,
  type LocalLlmModelInfo,
} from "@/bindings";

import { Alert } from "../../ui/Alert";
import {
  Dropdown,
  SettingContainer,
  SettingsGroup,
  Textarea,
} from "@/components/ui";
import { Button } from "../../ui/Button";
import { ResetButton } from "../../ui/ResetButton";
import { Input } from "../../ui/Input";

import { ProviderSelect } from "../PostProcessingSettingsApi/ProviderSelect";
import { BaseUrlField } from "../PostProcessingSettingsApi/BaseUrlField";
import { ApiKeyField } from "../PostProcessingSettingsApi/ApiKeyField";
import { ModelSelect } from "../PostProcessingSettingsApi/ModelSelect";
import { usePostProcessProviderState } from "../PostProcessingSettingsApi/usePostProcessProviderState";
import { ShortcutInput } from "../ShortcutInput";
import { PostProcessingToggle } from "../PostProcessingToggle";
import { useSettings } from "../../../hooks/useSettings";

type LocalLlmDownloadProgress = {
  model_id: string;
  downloaded: number;
  total: number;
  percentage: number;
};

const PostProcessingSettingsApiComponent: React.FC = () => {
  const { t } = useTranslation();
  const state = usePostProcessProviderState();
  const [credentialStoreStatus, setCredentialStoreStatus] =
    useState<CredentialStoreStatus | null>(null);
  const [credentialStoreStatusLoadFailed, setCredentialStoreStatusLoadFailed] =
    useState(false);
  const [sessionOnlyApiKey, setSessionOnlyApiKey] = useState(false);

  useEffect(() => {
    let isMounted = true;

    commands
      .getCredentialStoreStatus()
      .then((status) => {
        if (!isMounted) return;
        setCredentialStoreStatus(status);
        setCredentialStoreStatusLoadFailed(false);
      })
      .catch(() => {
        if (!isMounted) return;
        setCredentialStoreStatus(null);
        setCredentialStoreStatusLoadFailed(true);
      });

    return () => {
      isMounted = false;
    };
  }, []);

  return (
    <>
      <SettingContainer
        title={t("settings.postProcessing.api.provider.title")}
        description={t("settings.postProcessing.api.provider.description")}
        descriptionMode="tooltip"
        layout="horizontal"
        grouped={true}
      >
        <div className="flex items-center gap-2">
          <ProviderSelect
            options={state.providerOptions}
            value={state.selectedProviderId}
            onChange={state.handleProviderSelect}
          />
        </div>
      </SettingContainer>

      {!state.isAppleProvider && (
        <Alert variant="info" contained>
          {t("settings.postProcessing.api.dataFlowNotice", {
            provider: state.selectedProvider?.label ?? state.selectedProviderId,
          })}
        </Alert>
      )}

      {state.isAppleProvider ? (
        state.appleIntelligenceUnavailable ? (
          <Alert variant="error" contained>
            {t("settings.postProcessing.api.appleIntelligence.unavailable")}
          </Alert>
        ) : null
      ) : (
        <>
          {state.selectedProvider?.allow_base_url_edit && (
            <SettingContainer
              title={t("settings.postProcessing.api.baseUrl.title")}
              description={t("settings.postProcessing.api.baseUrl.description")}
              descriptionMode="tooltip"
              layout="horizontal"
              grouped={true}
            >
              <div className="flex items-center gap-2">
                <BaseUrlField
                  value={state.baseUrl}
                  onBlur={state.handleBaseUrlChange}
                  placeholder={t(
                    "settings.postProcessing.api.baseUrl.placeholder",
                  )}
                  disabled={state.isBaseUrlUpdating}
                  className="min-w-[380px]"
                />
              </div>
            </SettingContainer>
          )}

          <SettingContainer
            title={t("settings.postProcessing.api.apiKey.title")}
            description={t("settings.postProcessing.api.apiKey.description")}
            descriptionMode="tooltip"
            layout="horizontal"
            grouped={true}
          >
            <div className="flex items-center gap-2">
              <ApiKeyField
                value={state.apiKey}
                onBlur={(value) =>
                  state.handleApiKeyChange(value, sessionOnlyApiKey)
                }
                placeholder={t(
                  "settings.postProcessing.api.apiKey.placeholder",
                )}
                disabled={state.isApiKeyUpdating}
                className="min-w-[320px]"
              />
            </div>
            <label className="mt-3 flex items-start gap-2 text-sm text-mid-gray">
              <input
                type="checkbox"
                checked={sessionOnlyApiKey}
                onChange={(event) =>
                  setSessionOnlyApiKey(event.currentTarget.checked)
                }
                disabled={state.isApiKeyUpdating}
                className="mt-0.5 h-4 w-4 rounded-md border-mid-gray/50 accent-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
              />
              <span>{t("settings.postProcessing.api.apiKey.sessionOnly")}</span>
            </label>
            {credentialStoreStatusLoadFailed && (
              <Alert variant="warning" contained className="mt-3">
                {t("settings.postProcessing.api.apiKey.storeStatusUnknown")}
              </Alert>
            )}
            {credentialStoreStatus && (
              <Alert
                variant={
                  credentialStoreStatus.available ? "success" : "warning"
                }
                contained
                className="mt-3"
              >
                {credentialStoreStatus.available
                  ? t("settings.postProcessing.api.apiKey.storeAvailable", {
                      platform: credentialStoreStatus.platform,
                    })
                  : t(
                      credentialStoreStatus.retained_legacy_api_key_count > 0
                        ? "settings.postProcessing.api.apiKey.storeUnavailableLegacyRetained"
                        : "settings.postProcessing.api.apiKey.storeUnavailable",
                      {
                        platform: credentialStoreStatus.platform,
                        count:
                          credentialStoreStatus.retained_legacy_api_key_count,
                      },
                    )}
              </Alert>
            )}
          </SettingContainer>
        </>
      )}

      {!state.isAppleProvider && (
        <SettingContainer
          title={t("settings.postProcessing.api.model.title")}
          description={
            state.isCustomProvider
              ? t("settings.postProcessing.api.model.descriptionCustom")
              : t("settings.postProcessing.api.model.descriptionDefault")
          }
          descriptionMode="tooltip"
          layout="stacked"
          grouped={true}
        >
          <div className="flex items-center gap-2">
            <ModelSelect
              value={state.model}
              options={state.modelOptions}
              disabled={state.isModelUpdating}
              isLoading={state.isFetchingModels}
              ariaLabel={t("settings.postProcessing.api.model.title")}
              placeholder={
                state.modelOptions.length > 0
                  ? t(
                      "settings.postProcessing.api.model.placeholderWithOptions",
                    )
                  : t("settings.postProcessing.api.model.placeholderNoOptions")
              }
              onSelect={state.handleModelSelect}
              onCreate={state.handleModelCreate}
              onBlur={() => {}}
              className="flex-1 min-w-[380px]"
            />
            <ResetButton
              onClick={state.handleRefreshModels}
              disabled={state.isFetchingModels}
              ariaLabel={t("settings.postProcessing.api.model.refreshModels")}
              className="flex h-10 w-10 items-center justify-center"
            >
              <RefreshCcw
                className={`h-4 w-4 ${state.isFetchingModels ? "animate-spin" : ""}`}
              />
            </ResetButton>
          </div>
        </SettingContainer>
      )}
    </>
  );
};

const PostProcessingLocalModelComponent: React.FC = () => {
  const { t } = useTranslation();
  const { settings, refreshSettings } = useSettings();
  const [models, setModels] = useState<LocalLlmModelInfo[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [busyModelId, setBusyModelId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState<Record<string, number>>({});

  const localLlmSettings = settings?.local_llm;
  const selectedModelId = localLlmSettings?.selected_model_id ?? "";
  const localLlmEnabled = localLlmSettings?.enabled ?? false;
  const selectedModel = useMemo(
    () => models.find((model) => model.id === selectedModelId),
    [models, selectedModelId],
  );
  const canEnable = !!selectedModel?.is_downloaded;

  const loadModels = useCallback(async () => {
    setIsLoading(true);
    const result = await commands.listLocalLlmModels();
    if (result.status === "ok") {
      setModels(result.data);
      setError(null);
    } else {
      setError(result.error);
    }
    setIsLoading(false);
  }, []);

  useEffect(() => {
    void loadModels();

    let disposed = false;
    const unlisteners: Array<() => void> = [];

    void listen<LocalLlmDownloadProgress>(
      "local-llm-download-progress",
      (event) => {
        setProgress((current) => ({
          ...current,
          [event.payload.model_id]: event.payload.percentage,
        }));
      },
    ).then((unlisten) => {
      if (disposed) {
        unlisten();
      } else {
        unlisteners.push(unlisten);
      }
    });

    void listen<string>("local-llm-model-changed", () => {
      void loadModels();
    }).then((unlisten) => {
      if (disposed) {
        unlisten();
      } else {
        unlisteners.push(unlisten);
      }
    });

    return () => {
      disposed = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, [loadModels]);

  const runModelAction = async (
    modelId: string,
    action: () => Promise<
      { status: "ok"; data: unknown } | { status: "error"; error: string }
    >,
  ) => {
    setBusyModelId(modelId);
    setError(null);
    try {
      const result = await action();
      if (result.status === "error") {
        setError(result.error);
      }
      await loadModels();
      await refreshSettings();
    } finally {
      setBusyModelId(null);
    }
  };

  const handleDownload = (modelId: string) =>
    runModelAction(modelId, () => commands.downloadLocalLlmModel(modelId));

  const handleCancel = (modelId: string) =>
    runModelAction(modelId, () => commands.cancelLocalLlmDownload(modelId));

  const handleDelete = (modelId: string) =>
    runModelAction(modelId, () => commands.deleteLocalLlmModel(modelId));

  const handleSelect = (modelId: string) =>
    runModelAction(modelId, () => commands.selectLocalLlmModel(modelId));

  const handleEnabledChange = async (enabled: boolean) => {
    setBusyModelId("__enabled__");
    setError(null);
    try {
      const result = await commands.setLocalLlmEnabled(enabled);
      if (result.status === "error") {
        setError(result.error);
      }
      await refreshSettings();
      await loadModels();
    } finally {
      setBusyModelId(null);
    }
  };

  return (
    <SettingContainer
      title={t("settings.postProcessing.localModel.title")}
      description={t("settings.postProcessing.localModel.description")}
      descriptionMode="tooltip"
      layout="stacked"
      grouped={true}
    >
      <div className="space-y-3">
        <div className="flex items-center justify-between gap-3">
          <Alert variant="info" contained className="flex-1">
            {t("settings.postProcessing.localModel.caveat")}
          </Alert>
          <Button
            variant={localLlmEnabled ? "secondary" : "primary"}
            size="md"
            onClick={() => handleEnabledChange(!localLlmEnabled)}
            disabled={
              busyModelId === "__enabled__" || (!localLlmEnabled && !canEnable)
            }
          >
            {localLlmEnabled
              ? t("settings.postProcessing.localModel.disable")
              : t("settings.postProcessing.localModel.enable")}
          </Button>
        </div>

        {!canEnable && (
          <p className="text-xs text-mid-gray">
            {t("settings.postProcessing.localModel.enableUnavailable")}
          </p>
        )}

        {error && (
          <Alert variant="error" contained>
            {error}
          </Alert>
        )}

        <div className="space-y-2">
          {isLoading ? (
            <div className="flex items-center gap-2 text-sm text-mid-gray">
              <Loader2 className="h-4 w-4 animate-spin" />
              <span>{t("settings.postProcessing.localModel.loading")}</span>
            </div>
          ) : (
            models.map((model) => {
              const isSelected = model.id === selectedModelId;
              const isBusy = busyModelId === model.id || model.is_downloading;
              const modelProgress = progress[model.id] ?? 0;

              return (
                <div
                  key={model.id}
                  className="rounded-md border border-mid-gray/20 bg-mid-gray/5 p-3"
                >
                  <div className="flex items-start justify-between gap-3">
                    <div className="min-w-0 space-y-1">
                      <div className="flex flex-wrap items-center gap-2">
                        <span className="font-semibold text-sm">
                          {model.label}
                        </span>
                        {isSelected && (
                          <span className="rounded-md bg-accent/15 px-2 py-0.5 text-xs font-medium text-text">
                            {t("settings.postProcessing.localModel.selected")}
                          </span>
                        )}
                        <span className="rounded-md bg-mid-gray/10 px-2 py-0.5 text-xs text-mid-gray">
                          {t(
                            `settings.postProcessing.localModel.roles.${model.recommended_role}`,
                            model.recommended_role,
                          )}
                        </span>
                      </div>
                      <p className="text-xs text-mid-gray">
                        {t("settings.postProcessing.localModel.modelDetails", {
                          size: model.size_mb,
                          quantization: model.quantization,
                          license: model.license_label,
                        })}
                      </p>
                      <p className="text-xs text-mid-gray/80">
                        {model.supported_language_notes}
                      </p>
                      {model.is_downloading && (
                        <div className="h-1.5 overflow-hidden rounded-full bg-mid-gray/20">
                          <div
                            className="h-full rounded-full bg-accent"
                            style={{
                              width: `${Math.min(100, Math.max(0, modelProgress))}%`,
                            }}
                          />
                        </div>
                      )}
                    </div>

                    <div className="flex shrink-0 items-center gap-1">
                      {model.is_downloading ? (
                        <Button
                          variant="secondary"
                          size="sm"
                          onClick={() => handleCancel(model.id)}
                        >
                          {t("settings.postProcessing.localModel.cancel")}
                        </Button>
                      ) : model.is_downloaded ? (
                        <>
                          <Button
                            variant={isSelected ? "secondary" : "primary-soft"}
                            size="sm"
                            onClick={() => handleSelect(model.id)}
                            disabled={isSelected || isBusy}
                            aria-label={t(
                              "settings.postProcessing.localModel.select",
                            )}
                          >
                            {isSelected ? (
                              <Check className="h-4 w-4" />
                            ) : (
                              t("settings.postProcessing.localModel.select")
                            )}
                          </Button>
                          <Button
                            variant="danger-ghost"
                            size="sm"
                            onClick={() => handleDelete(model.id)}
                            disabled={isBusy}
                            aria-label={t(
                              "settings.postProcessing.localModel.delete",
                            )}
                          >
                            <Trash2 className="h-4 w-4" />
                          </Button>
                        </>
                      ) : (
                        <Button
                          variant="primary-soft"
                          size="sm"
                          onClick={() => handleDownload(model.id)}
                          disabled={isBusy}
                          className="inline-flex items-center gap-1"
                        >
                          <Download className="h-4 w-4" />
                          <span>
                            {t("settings.postProcessing.localModel.download")}
                          </span>
                        </Button>
                      )}
                    </div>
                  </div>
                </div>
              );
            })
          )}
        </div>
      </div>
    </SettingContainer>
  );
};

const PostProcessingSettingsPromptsComponent: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating, refreshSettings } =
    useSettings();
  const [isCreating, setIsCreating] = useState(false);
  const [draftName, setDraftName] = useState("");
  const [draftText, setDraftText] = useState("");

  const prompts = getSetting("post_process_prompts") || [];
  const selectedPromptId = getSetting("post_process_selected_prompt_id") || "";
  const selectedPrompt =
    prompts.find((prompt) => prompt.id === selectedPromptId) || null;

  useEffect(() => {
    if (isCreating) return;

    if (selectedPrompt) {
      setDraftName(selectedPrompt.name);
      setDraftText(selectedPrompt.prompt);
    } else {
      setDraftName("");
      setDraftText("");
    }
  }, [
    isCreating,
    selectedPromptId,
    selectedPrompt?.name,
    selectedPrompt?.prompt,
  ]);

  const handlePromptSelect = (promptId: string | null) => {
    if (!promptId) return;
    updateSetting("post_process_selected_prompt_id", promptId);
    setIsCreating(false);
  };

  const handleCreatePrompt = async () => {
    if (!draftName.trim() || !draftText.trim()) return;

    try {
      const result = await commands.addPostProcessPrompt(
        draftName.trim(),
        draftText.trim(),
      );
      if (result.status === "ok") {
        await refreshSettings();
        updateSetting("post_process_selected_prompt_id", result.data.id);
        setIsCreating(false);
      }
    } catch (error) {
      console.error("Failed to create prompt:", error);
    }
  };

  const handleUpdatePrompt = async () => {
    if (!selectedPromptId || !draftName.trim() || !draftText.trim()) return;

    try {
      await commands.updatePostProcessPrompt(
        selectedPromptId,
        draftName.trim(),
        draftText.trim(),
      );
      await refreshSettings();
    } catch (error) {
      console.error("Failed to update prompt:", error);
    }
  };

  const handleDeletePrompt = async (promptId: string) => {
    if (!promptId) return;

    try {
      await commands.deletePostProcessPrompt(promptId);
      await refreshSettings();
      setIsCreating(false);
    } catch (error) {
      console.error("Failed to delete prompt:", error);
    }
  };

  const handleCancelCreate = () => {
    setIsCreating(false);
    if (selectedPrompt) {
      setDraftName(selectedPrompt.name);
      setDraftText(selectedPrompt.prompt);
    } else {
      setDraftName("");
      setDraftText("");
    }
  };

  const handleStartCreate = () => {
    setIsCreating(true);
    setDraftName("");
    setDraftText("");
  };

  const hasPrompts = prompts.length > 0;
  const isDirty =
    !!selectedPrompt &&
    (draftName.trim() !== selectedPrompt.name ||
      draftText.trim() !== selectedPrompt.prompt.trim());

  return (
    <SettingContainer
      title={t("settings.postProcessing.prompts.selectedPrompt.title")}
      description={t(
        "settings.postProcessing.prompts.selectedPrompt.description",
      )}
      descriptionMode="tooltip"
      layout="stacked"
      grouped={true}
    >
      <div className="space-y-3">
        <div className="flex gap-2">
          <Dropdown
            selectedValue={selectedPromptId || null}
            options={prompts.map((p) => ({
              value: p.id,
              label: p.name,
            }))}
            onSelect={(value) => handlePromptSelect(value)}
            placeholder={
              prompts.length === 0
                ? t("settings.postProcessing.prompts.noPrompts")
                : t("settings.postProcessing.prompts.selectPrompt")
            }
            disabled={
              isUpdating("post_process_selected_prompt_id") || isCreating
            }
            className="flex-1"
          />
          <Button
            onClick={handleStartCreate}
            variant="primary"
            size="md"
            disabled={isCreating}
          >
            {t("settings.postProcessing.prompts.createNew")}
          </Button>
        </div>

        {!isCreating && hasPrompts && selectedPrompt && (
          <div className="space-y-3">
            <div className="space-y-2 flex flex-col">
              <label className="text-sm font-semibold">
                {t("settings.postProcessing.prompts.promptLabel")}
              </label>
              <Input
                type="text"
                value={draftName}
                onChange={(e) => setDraftName(e.target.value)}
                placeholder={t(
                  "settings.postProcessing.prompts.promptLabelPlaceholder",
                )}
                variant="compact"
              />
            </div>

            <div className="space-y-2 flex flex-col">
              <label className="text-sm font-semibold">
                {t("settings.postProcessing.prompts.promptInstructions")}
              </label>
              <Textarea
                value={draftText}
                onChange={(e) => setDraftText(e.target.value)}
                placeholder={t(
                  "settings.postProcessing.prompts.promptInstructionsPlaceholder",
                )}
              />
              <p className="text-xs text-mid-gray/70">
                <Trans
                  i18nKey="settings.postProcessing.prompts.promptTip"
                  components={{ code: <code /> }}
                />
              </p>
            </div>

            <div className="flex gap-2 pt-2">
              <Button
                onClick={handleUpdatePrompt}
                variant="primary"
                size="md"
                disabled={!draftName.trim() || !draftText.trim() || !isDirty}
              >
                {t("settings.postProcessing.prompts.updatePrompt")}
              </Button>
              <Button
                onClick={() => handleDeletePrompt(selectedPromptId)}
                variant="secondary"
                size="md"
                disabled={!selectedPromptId || prompts.length <= 1}
              >
                {t("settings.postProcessing.prompts.deletePrompt")}
              </Button>
            </div>
          </div>
        )}

        {!isCreating && !selectedPrompt && (
          <div className="p-3 bg-mid-gray/5 rounded-md border border-mid-gray/20">
            <p className="text-sm text-mid-gray">
              {hasPrompts
                ? t("settings.postProcessing.prompts.selectToEdit")
                : t("settings.postProcessing.prompts.createFirst")}
            </p>
          </div>
        )}

        {isCreating && (
          <div className="space-y-3">
            <div className="space-y-2 block flex flex-col">
              <label className="text-sm font-semibold text-text">
                {t("settings.postProcessing.prompts.promptLabel")}
              </label>
              <Input
                type="text"
                value={draftName}
                onChange={(e) => setDraftName(e.target.value)}
                placeholder={t(
                  "settings.postProcessing.prompts.promptLabelPlaceholder",
                )}
                variant="compact"
              />
            </div>

            <div className="space-y-2 flex flex-col">
              <label className="text-sm font-semibold">
                {t("settings.postProcessing.prompts.promptInstructions")}
              </label>
              <Textarea
                value={draftText}
                onChange={(e) => setDraftText(e.target.value)}
                placeholder={t(
                  "settings.postProcessing.prompts.promptInstructionsPlaceholder",
                )}
              />
              <p className="text-xs text-mid-gray/70">
                <Trans
                  i18nKey="settings.postProcessing.prompts.promptTip"
                  components={{ code: <code /> }}
                />
              </p>
            </div>

            <div className="flex gap-2 pt-2">
              <Button
                onClick={handleCreatePrompt}
                variant="primary"
                size="md"
                disabled={!draftName.trim() || !draftText.trim()}
              >
                {t("settings.postProcessing.prompts.createPrompt")}
              </Button>
              <Button
                onClick={handleCancelCreate}
                variant="secondary"
                size="md"
              >
                {t("settings.postProcessing.prompts.cancel")}
              </Button>
            </div>
          </div>
        )}
      </div>
    </SettingContainer>
  );
};

export const PostProcessingSettingsApi = React.memo(
  PostProcessingSettingsApiComponent,
);
PostProcessingSettingsApi.displayName = "PostProcessingSettingsApi";

export const PostProcessingSettingsPrompts = React.memo(
  PostProcessingSettingsPromptsComponent,
);
PostProcessingSettingsPrompts.displayName = "PostProcessingSettingsPrompts";

export const PostProcessingSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, settings, refreshSettings } = useSettings();
  const postProcessingEnabled = getSetting("post_process_enabled") || false;
  const localLlmEnabled = settings?.local_llm?.enabled ?? false;
  const hasSelectedLocalModel = !!settings?.local_llm?.selected_model_id;
  const [engineError, setEngineError] = useState<string | null>(null);
  const [isEngineUpdating, setIsEngineUpdating] = useState(false);

  const handleEngineSelect = async (engine: "api" | "local") => {
    const enableLocal = engine === "local";
    if (enableLocal === localLlmEnabled) return;

    setIsEngineUpdating(true);
    setEngineError(null);
    try {
      const result = await commands.setLocalLlmEnabled(enableLocal);
      if (result.status === "error") {
        setEngineError(result.error);
      }
      await refreshSettings();
    } finally {
      setIsEngineUpdating(false);
    }
  };

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <SettingsGroup title={t("settings.postProcessing.groups.enable")}>
        <PostProcessingToggle grouped descriptionMode="inline" />
      </SettingsGroup>

      {postProcessingEnabled && (
        <SettingsGroup title={t("settings.postProcessing.hotkey.title")}>
          <ShortcutInput
            shortcutId="transcribe_with_post_process"
            descriptionMode="tooltip"
            grouped={true}
          />
        </SettingsGroup>
      )}

      {postProcessingEnabled && (
        <SettingsGroup title={t("settings.postProcessing.engine.title")}>
          <SettingContainer
            title={t("settings.postProcessing.engine.modeTitle")}
            description={t("settings.postProcessing.engine.description")}
            descriptionMode="tooltip"
            layout="stacked"
            grouped={true}
          >
            <div className="space-y-2">
              <div className="grid grid-cols-2 gap-2">
                <Button
                  variant={localLlmEnabled ? "secondary" : "primary"}
                  size="md"
                  aria-pressed={!localLlmEnabled}
                  disabled={isEngineUpdating}
                  onClick={() => handleEngineSelect("api")}
                >
                  {t("settings.postProcessing.engine.api")}
                </Button>
                <Button
                  variant={localLlmEnabled ? "primary" : "secondary"}
                  size="md"
                  aria-pressed={localLlmEnabled}
                  disabled={isEngineUpdating || !hasSelectedLocalModel}
                  onClick={() => handleEngineSelect("local")}
                >
                  {t("settings.postProcessing.engine.local")}
                </Button>
              </div>
              <p className="text-xs leading-snug text-mid-gray">
                {localLlmEnabled
                  ? t("settings.postProcessing.engine.localActive")
                  : t("settings.postProcessing.engine.apiActive")}
              </p>
              {!hasSelectedLocalModel && (
                <p className="text-xs leading-snug text-mid-gray">
                  {t("settings.postProcessing.engine.localUnavailable")}
                </p>
              )}
              {engineError && (
                <Alert variant="error" contained>
                  {engineError}
                </Alert>
              )}
            </div>
          </SettingContainer>
        </SettingsGroup>
      )}

      {postProcessingEnabled && !localLlmEnabled && (
        <SettingsGroup title={t("settings.postProcessing.api.title")}>
          <PostProcessingSettingsApi />
        </SettingsGroup>
      )}

      {postProcessingEnabled && (
        <SettingsGroup
          title={t("settings.postProcessing.localModel.groupTitle")}
        >
          <PostProcessingLocalModelComponent />
        </SettingsGroup>
      )}

      {postProcessingEnabled && (
        <SettingsGroup title={t("settings.postProcessing.prompts.title")}>
          <PostProcessingSettingsPrompts />
        </SettingsGroup>
      )}
    </div>
  );
};
