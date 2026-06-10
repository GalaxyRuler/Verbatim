use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Clone, Debug, Serialize, Deserialize, Type, PartialEq, Eq)]
pub enum ShortcutIntent {
    Default,
    Raw,
    PostProcess,
    Profile(String),
}

#[derive(Clone, Debug, Serialize, Deserialize, Type, PartialEq, Eq)]
pub enum TargetKind {
    Email,
    CasualMessage,
    Technical,
    Notes,
    BrowserPrompt,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type, PartialEq, Eq)]
pub enum LanguageClass {
    Empty,
    MostlyArabic,
    MostlyLatin,
    Mixed,
    TechnicalMixed,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type, PartialEq)]
pub struct LanguageAnalysis {
    pub class: LanguageClass,
    pub shortlist: Vec<String>,
    pub arabic_ratio: f32,
    pub latin_ratio: f32,
    pub technical_token_count: usize,
    pub contains_url: bool,
    pub contains_identifier: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct CapturedContext {
    pub captured_at_ms: i64,
    pub process_name: Option<String>,
    pub window_title: Option<String>,
    pub window_title_hash: Option<String>,
    pub window_class: Option<String>,
    pub target_kind: TargetKind,
    pub target_fingerprint: Option<String>,
    pub is_sensitive: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct PreRouteDecision {
    pub candidate_profile_ids: Vec<String>,
    pub transcription_language_hint: Option<String>,
    pub reasons: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct RoutingDecision {
    pub profile_id: String,
    pub confidence: u8,
    pub reasons: Vec<String>,
    pub pre_route: PreRouteDecision,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type, PartialEq, Eq)]
pub enum InsertionMethod {
    None,
    Direct,
    Clipboard,
    ExternalScript,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct InsertionReceipt {
    pub attempted: bool,
    pub succeeded: bool,
    pub method: InsertionMethod,
    pub target_verified: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type, PartialEq)]
pub struct AdaptiveProcessResult {
    pub final_text: String,
    pub post_processed_text: Option<String>,
    pub post_process_prompt: Option<String>,
    pub language: LanguageAnalysis,
    pub routing: RoutingDecision,
}
