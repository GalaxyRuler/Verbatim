# Security Policy

## Supported Versions

Security fixes are prioritized for the latest published Verbatim release and
the current `main` branch. Older releases may receive fixes only when the issue
is severe and the fix can be backported safely.

## Reporting a Vulnerability

Use GitHub private vulnerability reporting or a draft security advisory if it is
available for this repository. If it is not available, open a minimal public
GitHub Discussion or issue that asks for a private security contact, but do not
include exploit details, private transcripts, recordings, logs, API keys, or
other sensitive material.

Include the following in the private report when possible:

- Affected Verbatim version and commit, if known.
- Operating system, architecture, and install type.
- Clear reproduction steps.
- Impact and what data or capability is exposed.
- Whether the issue affects local-only transcription, optional remote
  post-processing, update/download integrity, text insertion, clipboard
  handling, or filesystem access.
- Sanitized logs or screenshots only when they are necessary.

## Do Not Include Sensitive User Data

Do not attach raw recordings, transcript databases, full app-data directories,
API keys, credentials, private endpoints, or unredacted logs to public reports.
If a reproducer requires sensitive material, say so in the private report and
coordinate a safer reproduction path first.

## Response Expectations

Maintainers will try to acknowledge credible private reports promptly, reproduce
the issue, and coordinate a fix before public disclosure. Response time depends
on maintainer availability and severity.

## Release-Key or Signing-Key Compromise

If an updater signing key, code-signing certificate, GitHub Actions secret, or
release credential may be compromised:

1. Revoke or rotate the affected credential.
2. Disable affected release automation until rotation is complete.
3. Publish a security advisory describing the affected versions and artifacts.
4. Ship a fixed release signed with the rotated credential.
5. Document verification steps for users.

## Security Scope

Security-sensitive areas include:

- Desktop updater metadata, signatures, checksums, and release artifacts.
- Tauri capabilities, CSP, filesystem scopes, shell access, and custom protocol
  access.
- Clipboard and text insertion behavior that could paste private text into the
  wrong target.
- Transcript history, recordings, logs, and settings storage.
- Remote post-processing providers and API-key handling.
- Native shortcut, accessibility, microphone, and platform helper integrations.
