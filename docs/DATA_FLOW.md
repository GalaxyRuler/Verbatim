# Data Flow

This document describes where sensitive data moves through Verbatim.

## Audio

1. Microphone audio is captured by the Rust backend.
2. Voice activity detection filters the recording.
3. Local transcription engines process audio on the user's machine.
4. Depending on retention settings, WAV recordings may be saved under the app data recordings directory.

Audio is not sent to a network service by the current local transcription path.

## Transcript Text

1. Local transcription returns text to the backend.
2. The backend may apply filler filtering, dictionary learning, adaptive formatting, post-processing, transform actions, or insertion policy.
3. History may persist raw transcript text, post-processed text, transform source/result text, routing metadata, and recovery state.
4. Insertion may put text on the clipboard, type text through native APIs, or pass text to an external script through stdin.

Cancellation and private-session work must treat transcript text as sensitive until every side effect is either completed or explicitly blocked.

## Remote Text Providers

Remote post-processing or transform providers can receive transcript text, selected text, prompts, model IDs, and provider metadata. UI copy must make this explicit before users enable those providers.

Provider API keys are stored in the operating-system credential store and are not returned raw to the frontend.

## Clipboard

Clipboard insertion temporarily writes dictated text to the system clipboard. Restore logic must only restore the previous clipboard when the clipboard still contains Verbatim's own temporary payload. If the user or another app changes the clipboard during the paste delay, Verbatim must leave that newer clipboard content alone.

## History And Recordings

History entries can include sensitive text and metadata. Recording files can include sensitive audio. Retention controls must make clear what is stored and how to delete it.

The frontend can read recording files only from the narrowed recordings asset/fs scopes.

## External Scripts

External script insertion receives dictated text through stdin. Dictated text must not be passed as a command-line argument because process arguments can be visible to other local tools.

## Updater

The app fetches updater metadata from GitHub releases. `latest.json` includes updater URLs and signatures. Release assets are accompanied by `SHA256SUMS.txt` and `RELEASE_MANIFEST.json`.

## Logs And Diagnostics

Logs and diagnostics may include error codes, paths, model state, platform state, and timing. They must not include raw transcripts, prompts, API keys, selected text, or clipboard content.
