use crate::adaptive::profile::AdaptiveProfile;
use crate::settings::FormattingLevel;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PipelineFallbackReason {
    DeterministicValidation(String),
    LlmNoOutput,
    LlmValidation(String),
}

pub(crate) struct PipelineInput<'a> {
    pub raw_input: &'a str,
    pub selected_language: &'a str,
    pub formatting_level: FormattingLevel,
    pub adaptive_profile: Option<&'a AdaptiveProfile>,
    pub effective_language: Option<&'a str>,
    pub llm_output: Option<String>,
    pub llm_invoked: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct PipelinePreparationInput<'a> {
    pub raw_input: &'a str,
    pub selected_language: &'a str,
    pub formatting_level: FormattingLevel,
    pub adaptive_profile: Option<&'a AdaptiveProfile>,
    pub effective_language: Option<&'a str>,
}

pub(crate) struct PreparedPipeline<'a> {
    pub raw_input: String,
    converted_input: String,
    pub deterministic_output: String,
    pub zh_conversion_applied: bool,
    adaptive_profile: Option<&'a AdaptiveProfile>,
}

pub(crate) struct PipelineCompletion {
    pub llm_output: Option<String>,
    pub llm_invoked: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PipelineResult {
    pub raw_input: String,
    pub deterministic_output: String,
    pub llm_output: Option<String>,
    pub final_text: String,
    pub zh_conversion_applied: bool,
    pub llm_invoked: bool,
    pub fallback_reason: Option<PipelineFallbackReason>,
}

pub(crate) fn prepare_post_transcription_pipeline(
    input: PipelinePreparationInput<'_>,
) -> PreparedPipeline<'_> {
    let (converted_input, zh_conversion_applied) =
        convert_chinese_variant(input.selected_language, input.raw_input);

    let profile_output = match input.adaptive_profile {
        Some(profile) => crate::adaptive::processor::deterministic_process(
            &converted_input,
            profile,
            input.effective_language,
        ),
        _ => converted_input.clone(),
    };
    let deterministic_output = crate::adaptive::smart_formatting::format_transcript(
        &profile_output,
        input.formatting_level,
    );

    PreparedPipeline {
        raw_input: input.raw_input.to_string(),
        converted_input,
        deterministic_output,
        zh_conversion_applied,
        adaptive_profile: input.adaptive_profile,
    }
}

pub(crate) fn finalize_post_transcription_pipeline(
    prepared: PreparedPipeline<'_>,
    completion: PipelineCompletion,
) -> PipelineResult {
    let deterministic_validation = prepared
        .adaptive_profile
        .map(|profile| {
            crate::adaptive::processor::validate_output(
                &prepared.raw_input,
                &prepared.deterministic_output,
                profile,
            )
        })
        .unwrap_or(Ok(()));

    let (validated_deterministic, deterministic_fallback_reason) = match deterministic_validation {
        Ok(()) => (prepared.deterministic_output.clone(), None),
        Err(err) => (
            prepared.converted_input.clone(),
            Some(PipelineFallbackReason::DeterministicValidation(err)),
        ),
    };

    let mut final_text = validated_deterministic;
    let mut fallback_reason = deterministic_fallback_reason;
    if completion.llm_invoked {
        match completion.llm_output.as_deref() {
            Some(output) => {
                match validate_llm_output(
                    &prepared.deterministic_output,
                    output,
                    prepared.adaptive_profile,
                ) {
                    Ok(()) => {
                        final_text = output.to_string();
                        fallback_reason = None;
                    }
                    Err(err) => {
                        fallback_reason = Some(PipelineFallbackReason::LlmValidation(err));
                    }
                }
            }
            None => {
                if fallback_reason.is_none() {
                    fallback_reason = Some(PipelineFallbackReason::LlmNoOutput);
                }
            }
        }
    }

    PipelineResult {
        raw_input: prepared.raw_input,
        deterministic_output: prepared.deterministic_output,
        llm_output: completion.llm_output,
        final_text,
        zh_conversion_applied: prepared.zh_conversion_applied,
        llm_invoked: completion.llm_invoked,
        fallback_reason,
    }
}

pub(crate) fn run_post_transcription_pipeline(input: PipelineInput<'_>) -> PipelineResult {
    let prepared = prepare_post_transcription_pipeline(PipelinePreparationInput {
        raw_input: input.raw_input,
        selected_language: input.selected_language,
        formatting_level: input.formatting_level,
        adaptive_profile: input.adaptive_profile,
        effective_language: input.effective_language,
    });
    finalize_post_transcription_pipeline(
        prepared,
        PipelineCompletion {
            llm_output: input.llm_output,
            llm_invoked: input.llm_invoked,
        },
    )
}

fn convert_chinese_variant(selected_language: &str, input: &str) -> (String, bool) {
    let config = match selected_language {
        "zh-Hans" => ferrous_opencc::config::BuiltinConfig::Tw2sp,
        "zh-Hant" => ferrous_opencc::config::BuiltinConfig::S2tw,
        _ => return (input.to_string(), false),
    };

    match ferrous_opencc::OpenCC::from_config(config) {
        Ok(converter) => (converter.convert(input), true),
        Err(_) => (input.to_string(), false),
    }
}

fn validate_llm_output(
    pre_llm_text: &str,
    output: &str,
    adaptive_profile: Option<&AdaptiveProfile>,
) -> Result<(), String> {
    if let Some(profile) = adaptive_profile {
        validate_declared_profile_policies(pre_llm_text, output, profile)?;
        crate::adaptive::processor::validate_output(pre_llm_text, output, profile)?;
    }

    crate::text_processing::validate_preserved_text(pre_llm_text, output)
        .map_err(|err| err.to_string())
}

fn validate_declared_profile_policies(
    pre_llm_text: &str,
    output: &str,
    profile: &AdaptiveProfile,
) -> Result<(), String> {
    if profile.validation.preserve_numbers && numeric_runs(pre_llm_text) != numeric_runs(output) {
        return Err("preserve_numbers: numeric values were not preserved".to_string());
    }

    if profile.validation.preserve_raw_language {
        validate_script_family_preservation(pre_llm_text, output)?;
    }

    Ok(())
}

fn numeric_runs(text: &str) -> Vec<String> {
    let mut runs = Vec::new();
    let mut current = String::new();

    for ch in text.chars() {
        if ch.is_numeric() {
            current.push(ch);
        } else if !current.is_empty() {
            runs.push(std::mem::take(&mut current));
        }
    }

    if !current.is_empty() {
        runs.push(current);
    }

    runs
}

fn validate_script_family_preservation(pre_llm_text: &str, output: &str) -> Result<(), String> {
    let source_has_arabic = pre_llm_text.chars().any(is_arabic_script_char);
    let output_has_arabic = output.chars().any(is_arabic_script_char);
    if source_has_arabic && !output_has_arabic {
        return Err("preserve_raw_language: Arabic script was not preserved".to_string());
    }

    let source_has_latin = pre_llm_text.chars().any(|ch| ch.is_ascii_alphabetic());
    let output_has_latin = output.chars().any(|ch| ch.is_ascii_alphabetic());
    if source_has_latin && !output_has_latin {
        return Err("preserve_raw_language: Latin script was not preserved".to_string());
    }

    Ok(())
}

fn is_arabic_script_char(ch: char) -> bool {
    matches!(
        ch as u32,
        0x0600..=0x06FF | 0x0750..=0x077F | 0x08A0..=0x08FF | 0xFB50..=0xFDFF | 0xFE70..=0xFEFF
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_clean_profile() -> AdaptiveProfile {
        crate::adaptive::profile::default_profiles()
            .into_iter()
            .find(|profile| profile.id == "default_clean")
            .expect("default_clean profile")
    }

    #[test]
    fn pipeline_separates_deterministic_llm_and_final_outputs() {
        let result = run_post_transcription_pipeline(PipelineInput {
            raw_input: "hello world",
            selected_language: "en",
            formatting_level: FormattingLevel::Light,
            adaptive_profile: None,
            effective_language: Some("en"),
            llm_output: Some("Hello world.".to_string()),
            llm_invoked: true,
        });

        assert_eq!(result.raw_input, "hello world");
        assert_eq!(result.deterministic_output, "hello world");
        assert_eq!(result.llm_output.as_deref(), Some("Hello world."));
        assert_eq!(result.final_text, "Hello world.");
        assert!(result.llm_invoked);
        assert_eq!(result.fallback_reason, None);
    }

    #[test]
    fn pipeline_applies_chinese_conversion_before_adaptive_cleanup() {
        let profile = default_clean_profile();
        let result = run_post_transcription_pipeline(PipelineInput {
            raw_input: "um 軟件",
            selected_language: "zh-Hans",
            formatting_level: FormattingLevel::Light,
            adaptive_profile: Some(&profile),
            effective_language: Some("en"),
            llm_output: None,
            llm_invoked: false,
        });

        assert_eq!(result.deterministic_output, "软件");
        assert_eq!(result.final_text, "软件");
        assert!(result.zh_conversion_applied);
    }

    #[test]
    fn adaptive_cleanup_precedes_and_survives_disabled_smart_formatting() {
        let profile = default_clean_profile();
        let result = run_post_transcription_pipeline(PipelineInput {
            raw_input: "um hello",
            selected_language: "en",
            formatting_level: FormattingLevel::None,
            adaptive_profile: Some(&profile),
            effective_language: Some("en"),
            llm_output: None,
            llm_invoked: false,
        });

        assert_eq!(result.deterministic_output, "hello");
        assert_eq!(result.final_text, "hello");
    }

    #[test]
    fn rejected_llm_output_falls_back_to_deterministic_output_with_reason() {
        let result = run_post_transcription_pipeline(PipelineInput {
            raw_input: "The deadline is Monday actually Tuesday.",
            selected_language: "en",
            formatting_level: FormattingLevel::Light,
            adaptive_profile: None,
            effective_language: Some("en"),
            llm_output: Some("هذا موعد مختلف".to_string()),
            llm_invoked: true,
        });

        assert_eq!(result.deterministic_output, "The deadline is Tuesday.");
        assert_eq!(result.final_text, result.deterministic_output);
        assert!(matches!(
            result.fallback_reason,
            Some(PipelineFallbackReason::LlmValidation(_))
        ));
    }

    #[test]
    fn preserve_numbers_rejects_a_changed_numeric_run_and_falls_back() {
        let profile = default_clean_profile();
        let result = run_post_transcription_pipeline(PipelineInput {
            raw_input: "Send 42 files",
            selected_language: "en",
            formatting_level: FormattingLevel::None,
            adaptive_profile: Some(&profile),
            effective_language: Some("en"),
            llm_output: Some("Send 43 files".to_string()),
            llm_invoked: true,
        });

        assert_eq!(result.final_text, result.deterministic_output);
        let reason = match result.fallback_reason {
            Some(PipelineFallbackReason::LlmValidation(reason)) => reason,
            other => panic!("expected LLM validation fallback, got {other:?}"),
        };
        assert!(reason.contains("preserve_numbers"), "{reason}");
    }

    #[test]
    fn preserve_raw_language_rejects_script_loss_and_falls_back() {
        let mut profile = default_clean_profile();
        profile.validation.forbid_unrequested_translation = false;
        let result = run_post_transcription_pipeline(PipelineInput {
            raw_input: "هذا نص عربي واضح",
            selected_language: "ar",
            formatting_level: FormattingLevel::None,
            adaptive_profile: Some(&profile),
            effective_language: Some("ar"),
            llm_output: Some("This is clear English text".to_string()),
            llm_invoked: true,
        });

        assert_eq!(result.final_text, result.deterministic_output);
        let reason = match result.fallback_reason {
            Some(PipelineFallbackReason::LlmValidation(reason)) => reason,
            other => panic!("expected LLM validation fallback, got {other:?}"),
        };
        assert!(reason.contains("preserve_raw_language"), "{reason}");
    }
}
