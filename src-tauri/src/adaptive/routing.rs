use crate::adaptive::profile::AdaptiveProfile;
use crate::adaptive::types::{
    CapturedContext, LanguageAnalysis, LanguageClass, PreRouteDecision, RoutingDecision,
    ShortcutIntent, TargetKind,
};

pub fn route_before_recording(
    profiles: &[AdaptiveProfile],
    shortcut: ShortcutIntent,
    context: &CapturedContext,
    language_shortlist: &[String],
    default_profile_id: &str,
) -> PreRouteDecision {
    let mut candidate_profile_ids = Vec::new();
    let mut reasons = Vec::new();

    match shortcut {
        ShortcutIntent::Raw => {
            candidate_profile_ids.push("raw".to_string());
            reasons.push("raw shortcut selected".to_string());
        }
        ShortcutIntent::Profile(profile_id) => {
            candidate_profile_ids.push(profile_id.clone());
            reasons.push(format!("profile shortcut selected: {}", profile_id));
        }
        ShortcutIntent::PostProcess | ShortcutIntent::Default => {}
    }

    match context.target_kind {
        TargetKind::Email => candidate_profile_ids.push("email".to_string()),
        TargetKind::CasualMessage => candidate_profile_ids.push("default_clean".to_string()),
        TargetKind::Technical => candidate_profile_ids.push("technical".to_string()),
        TargetKind::Notes => candidate_profile_ids.push("notes_markdown".to_string()),
        TargetKind::BrowserPrompt | TargetKind::Unknown => {}
    }

    if candidate_profile_ids.is_empty() {
        candidate_profile_ids.push(default_profile_id.to_string());
        reasons.push(format!("default profile selected: {}", default_profile_id));
    } else {
        reasons.push(format!("target classified as {:?}", context.target_kind));
    }

    candidate_profile_ids.retain(|id| {
        if id == "translation" {
            return false;
        }

        profiles
            .iter()
            .any(|profile| profile.id == *id && profile.enabled)
    });
    candidate_profile_ids.dedup();

    PreRouteDecision {
        candidate_profile_ids,
        transcription_language_hint: transcription_hint(language_shortlist),
        reasons,
    }
}

pub fn route_after_transcription(
    profiles: &[AdaptiveProfile],
    pre_route: PreRouteDecision,
    context: &CapturedContext,
    language: &LanguageAnalysis,
    learned_profile_id: Option<&str>,
    default_profile_id: &str,
) -> RoutingDecision {
    let mut reasons = pre_route.reasons.clone();

    if pre_route.candidate_profile_ids.first().map(String::as_str) == Some("raw") {
        return RoutingDecision {
            profile_id: "raw".to_string(),
            confidence: 100,
            reasons,
            pre_route,
        };
    }

    let resolved_default_profile_id = profiles
        .iter()
        .any(|profile| {
            profile.id == default_profile_id && profile.enabled && profile.id != "translation"
        })
        .then_some(default_profile_id)
        .unwrap_or("default_clean");
    let mut profile_id = match context.target_kind {
        TargetKind::Email => "email",
        TargetKind::Technical => "technical",
        TargetKind::Notes => "notes_markdown",
        TargetKind::CasualMessage => resolved_default_profile_id,
        TargetKind::BrowserPrompt => "default_clean",
        TargetKind::Unknown => resolved_default_profile_id,
    }
    .to_string();

    if matches!(
        language.class,
        LanguageClass::Mixed | LanguageClass::TechnicalMixed
    ) && context.target_kind == TargetKind::Unknown
    {
        profile_id = "mixed_multilingual".to_string();
        reasons.push("mixed transcript with unknown target".to_string());
    }

    if language.class == LanguageClass::TechnicalMixed && context.target_kind != TargetKind::Email {
        profile_id = "technical".to_string();
        reasons.push("technical transcript signals".to_string());
    }

    if let Some(learned) = learned_profile_id {
        if learned != "translation"
            && profiles
                .iter()
                .any(|profile| profile.id == learned && profile.enabled)
        {
            profile_id = learned.to_string();
            reasons.push(format!("learned profile preference: {}", learned));
        }
    }

    if !profiles
        .iter()
        .any(|profile| profile.id == profile_id && profile.enabled && profile.id != "translation")
    {
        profile_id = "default_clean".to_string();
        reasons.push("selected profile unavailable; using default_clean".to_string());
    }

    let confidence = if reasons.iter().any(|reason| reason.contains("shortcut")) {
        100
    } else if context.target_kind != TargetKind::Unknown {
        80
    } else if profile_id == "mixed_multilingual" || profile_id == "technical" {
        70
    } else {
        50
    };

    RoutingDecision {
        profile_id,
        confidence,
        reasons,
        pre_route,
    }
}

fn transcription_hint(shortlist: &[String]) -> Option<String> {
    if shortlist.len() == 1 {
        shortlist.first().cloned()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adaptive::language::analyze_language;
    use crate::adaptive::profile::default_profiles;

    fn context(target_kind: TargetKind, process: &str) -> CapturedContext {
        CapturedContext {
            captured_at_ms: 1,
            process_name: Some(process.to_string()),
            window_title: None,
            window_title_hash: None,
            window_class: None,
            target_kind,
            target_fingerprint: Some(process.to_lowercase()),
            is_sensitive: false,
        }
    }

    #[test]
    fn raw_shortcut_wins_before_context() {
        let profiles = default_profiles();
        let ctx = context(TargetKind::Email, "OUTLOOK.EXE");
        let pre = route_before_recording(
            &profiles,
            ShortcutIntent::Raw,
            &ctx,
            &["en".to_string(), "ar".to_string()],
            "default_clean",
        );
        let language = analyze_language(
            "This should stay raw",
            &["en".to_string(), "ar".to_string()],
        );
        let post =
            route_after_transcription(&profiles, pre, &ctx, &language, None, "default_clean");

        assert_eq!(post.profile_id, "raw");
        assert!(post
            .reasons
            .iter()
            .any(|reason| reason.contains("shortcut")));
    }

    #[test]
    fn outlook_routes_to_email() {
        let profiles = default_profiles();
        let ctx = context(TargetKind::Email, "OUTLOOK.EXE");
        let pre = route_before_recording(
            &profiles,
            ShortcutIntent::Default,
            &ctx,
            &["en".to_string(), "ar".to_string()],
            "default_clean",
        );
        let language = analyze_language(
            "Please send the attached file today",
            &["en".to_string(), "ar".to_string()],
        );
        let post =
            route_after_transcription(&profiles, pre, &ctx, &language, None, "default_clean");

        assert_eq!(post.profile_id, "email");
        assert!(post.confidence >= 70);
    }

    #[test]
    fn technical_context_beats_mostly_latin_text() {
        let profiles = default_profiles();
        let ctx = context(TargetKind::Technical, "Code.exe");
        let pre = route_before_recording(
            &profiles,
            ShortcutIntent::Default,
            &ctx,
            &["en".to_string(), "ar".to_string()],
            "default_clean",
        );
        let language = analyze_language(
            "open the config and run cargo test",
            &["en".to_string(), "ar".to_string()],
        );
        let post =
            route_after_transcription(&profiles, pre, &ctx, &language, None, "default_clean");

        assert_eq!(post.profile_id, "technical");
    }

    #[test]
    fn mixed_language_routes_to_mixed_profile_when_context_unknown() {
        let profiles = default_profiles();
        let ctx = context(TargetKind::Unknown, "unknown.exe");
        let pre = route_before_recording(
            &profiles,
            ShortcutIntent::Default,
            &ctx,
            &["en".to_string(), "ar".to_string()],
            "default_clean",
        );
        let language = analyze_language(
            "خلينا send it tomorrow",
            &["en".to_string(), "ar".to_string()],
        );
        let post =
            route_after_transcription(&profiles, pre, &ctx, &language, None, "default_clean");

        assert_eq!(post.profile_id, "mixed_multilingual");
    }

    #[test]
    fn learned_translation_profile_is_ignored() {
        let profiles = default_profiles();
        let ctx = context(TargetKind::Unknown, "unknown.exe");
        let pre = route_before_recording(
            &profiles,
            ShortcutIntent::Default,
            &ctx,
            &["en".to_string(), "ar".to_string()],
            "default_clean",
        );
        let language = analyze_language("هذا نص عربي واضح", &["en".to_string(), "ar".to_string()]);
        let post = route_after_transcription(
            &profiles,
            pre,
            &ctx,
            &language,
            Some("translation"),
            "default_clean",
        );

        assert_eq!(post.profile_id, "default_clean");
    }

    #[test]
    fn unknown_plain_text_routes_to_default_profile() {
        let profiles = default_profiles();
        let ctx = context(TargetKind::Unknown, "unknown.exe");
        let pre = route_before_recording(
            &profiles,
            ShortcutIntent::Default,
            &ctx,
            &["en".to_string(), "ar".to_string()],
            "default_clean",
        );
        let language = analyze_language(
            "please send the file tomorrow",
            &["en".to_string(), "ar".to_string()],
        );
        let post =
            route_after_transcription(&profiles, pre, &ctx, &language, None, "default_clean");

        assert_eq!(post.profile_id, "default_clean");
        assert_eq!(post.confidence, 50);
    }
    #[test]
    fn unknown_plain_text_routes_to_configured_default_profile() {
        let profiles = default_profiles();
        let ctx = context(TargetKind::Unknown, "unknown.exe");
        let pre = route_before_recording(
            &profiles,
            ShortcutIntent::Default,
            &ctx,
            &["en".to_string(), "ar".to_string()],
            "notes_markdown",
        );
        let language = analyze_language(
            "please send the file tomorrow",
            &["en".to_string(), "ar".to_string()],
        );
        let post =
            route_after_transcription(&profiles, pre, &ctx, &language, None, "notes_markdown");

        assert_eq!(post.profile_id, "notes_markdown");
        assert_eq!(post.confidence, 50);
    }
    #[test]
    fn disabled_configured_default_falls_back_to_default_clean() {
        let mut profiles = default_profiles();
        profiles
            .iter_mut()
            .find(|profile| profile.id == "notes_markdown")
            .expect("notes profile")
            .enabled = false;
        let ctx = context(TargetKind::Unknown, "unknown.exe");
        let pre = route_before_recording(
            &profiles,
            ShortcutIntent::Default,
            &ctx,
            &["en".to_string(), "ar".to_string()],
            "notes_markdown",
        );
        let language = analyze_language(
            "please send the file tomorrow",
            &["en".to_string(), "ar".to_string()],
        );
        let post =
            route_after_transcription(&profiles, pre, &ctx, &language, None, "notes_markdown");

        assert_eq!(post.profile_id, "default_clean");
    }
}
