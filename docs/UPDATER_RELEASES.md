# Updater Releases

Verbatim uses the Tauri 2 updater for installed desktop builds. The app checks:

```text
https://github.com/GalaxyRuler/Verbatim/releases/latest/download/latest.json
```

The release workflow must publish `latest.json` and matching `.sig` files for updater-enabled releases.

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
4. Confirm the release assets include:
   - `latest.json`
   - at least one `.sig` file
   - platform installers for Windows, macOS, and Linux
5. Install the previous release and verify it detects the new release from the footer update control.

## Manual QA

Use a two-version test:

1. Install version `X`.
2. Publish version `X+1`.
3. Launch version `X`.
4. Confirm the footer shows an update.
5. Click update.
6. Confirm the update downloads, installs, relaunches, and the About screen shows `X+1`.

Portable installs are not expected to auto-update. The app should show the manual portable-update message.
