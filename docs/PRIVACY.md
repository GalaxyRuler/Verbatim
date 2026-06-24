# Privacy and Data Flow

Last updated: 2026-06-21

Verbatim is local-first speech-to-text software. Audio transcription is designed
to run on your device, but the app is not a zero-network application. Network
activity can occur for update checks, model downloads, and optional
post-processing providers that you configure.

## Summary

- Audio transcription runs locally with the selected on-device model.
- Update checks are enabled by default and contact GitHub Releases.
- Model downloads contact GitHub, Hugging Face, or Verbatim asset hosting,
  depending on the selected model.
- AI post-processing is off by default. If you enable a remote provider,
  transcript text or selected text is sent to that provider.
- Transcription history and recordings are stored locally unless deleted by the
  retention settings or by the user.
- API keys are redacted from debug output, but they are currently stored in the
  normal app settings store rather than an operating-system credential manager.

## Local Storage

Verbatim stores application data in the platform app-data directory. Portable
builds store the same data under the portable `Data/` directory.

Typical installed locations:

| Platform | App data location                                         |
| -------- | --------------------------------------------------------- |
| Windows  | `%APPDATA%\com.galaxyruler.verbatim\`                     |
| macOS    | `~/Library/Application Support/com.galaxyruler.verbatim/` |
| Linux    | `~/.config/com.galaxyruler.verbatim/`                     |

| Data class       | Location                          | Contents                                                                                                                             | Default retention                                                                          |
| ---------------- | --------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------ |
| Settings         | `settings_store.json`             | Preferences, shortcuts, model selection, provider configuration, custom words, snippets, API-key map                                 | Kept until changed or app data is deleted                                                  |
| History database | `history.db`                      | Transcript text, formatted/post-processed text, transform metadata, prompts used for post-processing, and redacted adaptive metadata | Unsaved recording-backed entries are pruned by the history limit                           |
| Recordings       | `recordings/`                     | WAV files associated with history entries                                                                                            | `Auto-Delete Recordings` defaults to keeping the latest history limit, which defaults to 5 |
| Models           | `models/` and local model folders | Downloaded transcription and local post-processing model files                                                                       | Kept until deleted                                                                         |
| Logs             | `logs/`                           | Diagnostic logs, errors, model/device state, and troubleshooting detail                                                              | Rotated by the logging plugin                                                              |

Logs should not intentionally contain transcript text, learned phrases, or API
keys. Still review logs before sharing them because they can include device
names, paths, operating-system details, provider names, model names, and error
messages.

## Network Activity

| Feature                         | Default               | Destination                                                                                                                             | Data sent                                                                                                                   |
| ------------------------------- | --------------------- | --------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| Update checks                   | On                    | GitHub Releases `latest.json` endpoint                                                                                                  | App version and normal HTTPS request metadata                                                                               |
| Installer/download links        | User initiated        | GitHub Releases                                                                                                                         | Normal HTTPS request metadata                                                                                               |
| Model downloads                 | User initiated        | Hugging Face and `verbatim-assets.galaxyruler.space`                                                                                    | Requested model URL and normal HTTPS request metadata                                                                       |
| Managed local model downloads   | User initiated        | Verbatim asset hosting, if a local model is downloaded                                                                                  | Requested model URL and normal HTTPS request metadata                                                                       |
| Remote post-processing          | Off                   | The selected provider, such as OpenAI-compatible endpoints, Anthropic, Groq, Cerebras, OpenRouter, Bedrock Mantle, or a custom base URL | Transcript text or selected text, prompt/template text, model/provider identifiers, and the provider API key in the request |
| Local post-processing endpoints | Off unless configured | Localhost endpoints such as LM Studio, Ollama, or vLLM                                                                                  | Transcript text or selected text sent to the local endpoint                                                                 |

Verbatim does not currently include telemetry or analytics. If analytics are
added later, they must be opt-in and documented separately.

## Clipboard and Text Insertion

When clipboard insertion is selected, Verbatim writes the transcript to the
clipboard, sends the paste shortcut, and then attempts to restore the previous
clipboard when configured to do so. If automatic insertion fails, Verbatim may
leave or copy the transcript to the clipboard so the user can paste it manually.

Clipboard contents are owned by the operating system and other applications can
observe or change them according to the platform's normal clipboard behavior.

## Per-Application Exclusions

Verbatim checks the foreground application against configured private-app
patterns before starting recording. If the target matches a private-app
exclusion, Verbatim blocks recording before microphone capture and clears the
captured target context for that shortcut. The default exclusions cover common
password managers such as 1Password, Bitwarden, and KeePass.

This guard is separate from private session. Private session suppresses history
and recording retention for the active session; per-application exclusions stop
recording before audio capture for matching foreground targets.

## History and Recordings

The History view can contain the raw transcript, corrected text, transformed
selected text, and a linked audio recording. Deleting a history entry deletes
the matching recording file when one exists. Saved entries are protected from
automatic cleanup.

For the lowest-retention workflow available today:

1. Open Settings > Advanced > History.
2. Set `History Limit` to `0`.
3. Set `Auto-Delete Recordings` to `Keep latest 0`.
4. Delete existing entries from Settings > History.
5. Avoid remote post-processing providers when dictating sensitive text.

This is not a dedicated private-session mode. A true no-history/no-recording
mode is still a separate product requirement.

## API Keys

Remote-provider API keys are stored in the settings store and redacted from
debug formatting. They are not currently stored in Windows Credential Manager,
macOS Keychain, Secret Service, or KWallet. Treat app-data backups and exported
diagnostics as sensitive until you have reviewed them.

## Uninstall Behavior

Uninstallers generally remove application binaries, not every app-data file.
If you want to remove local transcripts, recordings, models, settings, logs, or
API keys, delete the app-data directory after uninstalling.

## Sharing Diagnostics

Before sharing logs, screenshots, recordings, or a database file:

- Do not attach private audio or transcripts unless you intentionally want them
  reviewed.
- Remove API keys, bearer tokens, private endpoints, and usernames from logs.
- Prefer a short reproduction, OS version, app version, model name, paste method,
  and relevant error message over a full data-directory archive.
