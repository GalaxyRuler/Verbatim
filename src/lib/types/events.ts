export interface ModelStateEvent {
  event_type: string;
  model_id?: string;
  model_name?: string;
  error?: string;
  diagnostic_code?: string;
  fallback?: string;
}

export interface RecordingErrorEvent {
  error_type: string;
  detail?: string;
}
