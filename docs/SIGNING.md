# Signing

This project uses two different signing systems:

- Tauri updater signing proves update payload integrity to installed Verbatim apps.
- Platform code signing improves operating-system trust for installers and app bundles.

Never commit private keys, certificates, passwords, notarization credentials, or copied secret values.

## Tauri Updater Signing

The public updater key is stored in `src-tauri/tauri.conf.json` at `plugins.updater.pubkey`.

Required GitHub secrets:

- `TAURI_SIGNING_PRIVATE_KEY`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`

Generate updater keys outside the repository:

```bash
bun tauri signer generate
```

The release workflow requires updater signing secrets whenever updater artifacts are created.

## Windows Authenticode

Windows signing is controlled by the reusable build workflow when `sign-binaries` is true.

Currently wired GitHub secrets:

- `AZURE_CLIENT_ID`
- `AZURE_CLIENT_SECRET`
- `AZURE_TENANT_ID`

Required decision before production signing:

- Choose the certificate provider and approval process.
- Decide whether signing uses Azure Trusted Signing or a hardware/cloud HSM-backed certificate.
- Document the public publisher name exactly as it appears in Windows signature details.
- Enter the public publisher/signing identity in the release workflow's `signing_identity_label` input for signed releases.

Expected signed outputs:

- NSIS setup executable
- MSI installer
- Installed app executable
- Uninstaller executable, when produced by the installer toolchain

Verification command on Windows:

```powershell
Get-AuthenticodeSignature .\Verbatim_<version>_x64-setup.exe
Get-AuthenticodeSignature .\Verbatim_<version>_x64_en-US.msi
```

The signature status must be `Valid`, and the timestamp must be present for production releases.
When `sign-binaries` is true on Windows, CI fails if required wired signing
secrets are missing or any produced `.exe`/`.msi` lacks a valid Authenticode
signature and timestamp.

## macOS Developer ID And Notarization

macOS signing is controlled by the reusable build workflow when `sign-binaries` is true.

Required GitHub secrets:

- `APPLE_CERTIFICATE`
- `APPLE_CERTIFICATE_PASSWORD`
- `KEYCHAIN_PASSWORD`
- `APPLE_ID`
- `APPLE_ID_PASSWORD` or `APPLE_PASSWORD`
- `APPLE_TEAM_ID`

The public Developer ID identity used for the release must be included in the
release workflow's `signing_identity_label` input for signed releases.

Expected signed outputs:

- `.app` bundle and nested binaries
- `.dmg` distribution image

Static release configuration guardrails:

- `bundle.macOS.hardenedRuntime` must stay enabled in `src-tauri/tauri.conf.json`.
- `bundle.macOS.minimumSystemVersion` must stay set.
- `bundle.macOS.entitlements` must point at the reviewed entitlements plist.
- The reviewed entitlements plist must include microphone and audio-input access.
- Release configuration must not enable debug or broad code-loading entitlements such as `get-task-allow`, `allow-dyld-environment-variables`, or `disable-library-validation`.

These guardrails are enforced by `bun run check:tauri-security`.

Verification commands on macOS:

```bash
codesign --verify --deep --strict --verbose=2 Verbatim.app
spctl --assess --type execute --verbose Verbatim.app
spctl --assess --type open --verbose Verbatim_<version>_aarch64.dmg
```

Production releases must be notarized and stapled before publication.
When `sign-binaries` is true on macOS, CI fails if required signing secrets are
missing, the `.app` does not pass strict `codesign` verification, Gatekeeper
assessment fails, or the `.app`/`.dmg` does not validate as stapled.

After downloading release assets, validate signed-release manifest evidence:

```bash
bun run check:signed-release-evidence -- --dir <downloaded-release-assets>
```

This verifies that `RELEASE_MANIFEST.json`, `SHA256SUMS.txt`, and `latest.json`
agree on checksums, updater signatures, signing status, public signing identity,
SBOM links, and provenance links. It does not replace platform verification
commands such as `Get-AuthenticodeSignature`, `codesign`, `spctl`, or
`xcrun stapler validate`.

After clean-machine install smoke, validate signed install behavior evidence:

```bash
bun run check:install-smoke-signed-release-evidence -- --dir <install-smoke-evidence-dir>
```

This additionally requires macOS Gatekeeper evidence from a clean machine.

## Linux Packages

Linux `.deb` packages do not use Authenticode or Apple Developer ID. Trust comes from release checksums, updater signatures, and package provenance.

Before publishing, verify:

- The `.deb` installs on a clean supported Ubuntu system.
- Runtime shared libraries required by the package are present.
- `SHA256SUMS.txt` and `RELEASE_MANIFEST.json` include the package.

## Build Provenance Attestations

Release builds generate GitHub Artifact Attestations for packaged desktop
artifacts from the reusable build workflow. The attested subjects are the local
bundle outputs before publication: Windows `.exe`/`.msi`, macOS `.dmg`, and
Linux `.deb` files. These attestations are signed by GitHub's OIDC/Sigstore
attestation service and are separate from platform code-signing certificates.

Verification example:

```bash
gh attestation verify Verbatim_<version>_x64-setup.exe -R GalaxyRuler/Verbatim
gh attestation verify Verbatim_<version>_aarch64.dmg -R GalaxyRuler/Verbatim
gh attestation verify Verbatim_<version>_amd64.deb -R GalaxyRuler/Verbatim
```

Retain one verification status file per packaged desktop artifact at
`<downloaded-release-assets>/attestations/<asset>.attestation.json`:

```json
{
  "asset": "Verbatim_<version>_x64-setup.exe",
  "verified": true,
  "repository": "GalaxyRuler/Verbatim",
  "command": "gh attestation verify Verbatim_<version>_x64-setup.exe -R GalaxyRuler/Verbatim",
  "verified_at": "2026-06-23T12:00:00Z",
  "subject_sha256": "<sha256 from RELEASE_MANIFEST.json>"
}
```

Validate retained attestation evidence before approving an attested release:

```bash
bun run check:attested-release-evidence -- --dir <downloaded-release-assets>
bun run check:signed-attested-release-evidence -- --dir <downloaded-release-assets>
```

Artifact attestations do not replace Authenticode, Developer ID, notarization,
updater signatures, or release checksums. They prove where and how the release
artifact was built in GitHub Actions.

## Release Notes

The release body must state whether signing was enabled. Signed releases require
the public `signing_identity_label` workflow input so `RELEASE_MANIFEST.json` and
the release notes name the intended signing identity. If any platform is unsigned
or not notarized, the release must say so directly.
