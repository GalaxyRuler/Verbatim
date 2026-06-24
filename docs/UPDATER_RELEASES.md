# Updater Releases

Verbatim uses the Tauri 2 updater for installed desktop builds. The app checks:

```text
https://github.com/GalaxyRuler/Verbatim/releases/latest/download/latest.json
```

The release workflow must publish `latest.json`, matching updater `.sig` files,
the expected desktop installers, `SHA256SUMS.txt`, and `RELEASE_MANIFEST.json`
for every release.

## Signing Keys

Tauri updater signing is separate from Apple Developer ID and Windows trusted code signing.

- Public key: stored in `src-tauri/tauri.conf.json` under `plugins.updater.pubkey`.
- Private key: stored only in GitHub Actions secret `TAURI_SIGNING_PRIVATE_KEY`.
- Private key password: stored only in GitHub Actions secret `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.

Do not commit the private key, private key password, generated key files, or copied secret values.

## Generate A Key Pair

Run this outside the repository or in a private temporary directory:

```bash
bun tauri signer generate
```

Save the generated private key and password in a password manager. Put the public key into `src-tauri/tauri.conf.json`.

## Required GitHub Secrets

Set these repository secrets:

```text
TAURI_SIGNING_PRIVATE_KEY
TAURI_SIGNING_PRIVATE_KEY_PASSWORD
```

The release workflow fails before publishing if these are missing.

## Release Flow

1. Bump the app version in:
   - `package.json`
   - `src-tauri/Cargo.toml`
   - `src-tauri/tauri.conf.json`
2. Run local checks:
   - `bun run format:check`
   - `bun run lint`
   - `bun run build`
3. Trigger the `Release` workflow.
   - For signed releases, set `sign_binaries: true` and provide the public
     `signing_identity_label` that should appear in the release manifest and
     release notes.
   - For unsigned previews, leave `sign_binaries: false`; release notes will
     call out the unsigned status.
4. Confirm the release assets include:
   - `latest.json`
   - updater entries and signatures for Windows x64, macOS Apple Silicon, and Linux x64
   - updater `.sig` files for every desktop platform
   - platform installers for Windows, macOS, and Linux
   - `SHA256SUMS.txt`
   - `RELEASE_MANIFEST.json`
5. Install the previous release and verify it detects the new release from the footer update control.
6. Save updater smoke evidence JSON and run `bun run check:updater-smoke-evidence -- --dir <updater-smoke-evidence-dir>`.

## Release Asset Gate

The `Release` workflow finalizer fails if any expected desktop installer is
missing:

- `Verbatim_<version>_x64-setup.exe`
- `Verbatim_<version>_x64_en-US.msi`
- `Verbatim_<version>_aarch64.dmg`
- `Verbatim_<version>_amd64.deb`

It also parses `latest.json` and requires updater entries with signatures for:

- `windows-x86_64`
- `darwin-aarch64`
- `linux-x86_64`

The workflow uploads `SHA256SUMS.txt` after all artifacts are present. Re-running
the finalizer replaces the previous manifest so it matches the current release
asset set.

The workflow also uploads `RELEASE_MANIFEST.json` with file sizes, SHA-256
digests, updater platform keys, updater signature presence, signing status, and
reserved provenance/SBOM fields for every asset. Signed releases also record the
public signing identity supplied through `signing_identity_label`.

## Manual QA

Use a two-version test:

1. Install version `X`.
2. Publish version `X+1`.
3. Launch version `X`.
4. Confirm the footer shows an update.
5. Click update.
6. Confirm the update downloads, installs, relaunches, and the About screen shows `X+1`.

Portable installs are not expected to auto-update. The app should show the manual portable-update message.

## Updater Smoke Evidence

Retain one JSON file per platform in the updater-smoke evidence directory. File
names must match `updater-smoke*.json`, for example
`updater-smoke-windows-x86_64.json`.

```json
{
  "schema_version": 1,
  "platform": "windows-x86_64",
  "previous_version": "0.8.7",
  "target_version": "0.8.8",
  "previous_install_verified": true,
  "update_detected": true,
  "update_downloaded": true,
  "updater_signature_verified": true,
  "update_applied": true,
  "relaunched_version": "0.8.8",
  "clean_profile_preserved": true,
  "latest_json_url": "https://github.com/GalaxyRuler/Verbatim/releases/latest/download/latest.json",
  "updater_archive_name": "Verbatim_0.8.8_x64-setup.nsis.zip",
  "failures": []
}
```

For release approval, require evidence from every supported desktop updater
platform:

```bash
bun run check:updater-smoke-release-evidence -- --dir <updater-smoke-evidence-dir>
```

The checker fails if a platform is missing, if the previous and target versions
match, if the app did not relaunch into the target version, if updater signature
verification is not recorded, or if the smoke evidence recorded any failure.

## Clean-Machine Install Smoke Evidence

Retain one JSON file per platform in the install-smoke evidence directory. File
names must match `install-smoke*.json`, for example
`install-smoke-windows-x86_64.json`.

```json
{
  "schema_version": 1,
  "platform": "windows-x86_64",
  "version": "0.8.8",
  "artifact_name": "Verbatim_0.8.8_x64-setup.exe",
  "clean_machine": true,
  "install_verified": true,
  "launch_verified": true,
  "local_transcription_verified": true,
  "plain_text_insertion_verified": true,
  "uninstall_verified": true,
  "app_removed_after_uninstall": true,
  "app_data_policy_checked": true,
  "windows_default_uninstall_preserved_app_data": true,
  "windows_delete_app_data_removed_app_data": true,
  "trust_behavior_matches_release_notes": true,
  "failures": []
}
```

For release approval, require evidence from every supported desktop installer
platform:

```bash
bun run check:install-smoke-release-evidence -- --dir <install-smoke-evidence-dir>
```

For signed releases, use the signed gate:

```bash
bun run check:install-smoke-signed-release-evidence -- --dir <install-smoke-evidence-dir>
```

The checker fails if installation, launch, local transcription, plain-text
insertion, uninstall, app removal, app-data policy, or release-note trust
behavior evidence is missing. Windows evidence must cover default app-data
preservation and explicit app-data deletion. Signed macOS evidence must include
Gatekeeper verification from a clean machine.
