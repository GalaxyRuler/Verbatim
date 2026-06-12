# Versioning Policy

Verbatim uses stable Semantic Versioning for app, package, installer, updater, and release tags.

## Format

Use `MAJOR.MINOR.PATCH`.

Examples:

- `0.8.7`
- `0.9.0`
- `1.0.0`

Do not use fixed-width padding such as `0.08.007`. Leading zeroes are not SemVer-compatible and can break package managers, Cargo, Tauri, release tooling, and updater comparisons.

Release tags must be `vMAJOR.MINOR.PATCH`, for example `v0.8.7`.

## Bump Rules

Until Verbatim reaches `1.0.0`:

- Patch bump: bug fixes, crash fixes, installer fixes, updater fixes, copy changes, docs-only release polish, and small UI corrections.
- Minor bump: new user-facing capabilities, new settings, new workflows, model/provider support, storage migrations, or meaningful UX behavior changes.
- Major bump: reserve for `1.0.0` or a deliberate breaking change in settings, history data, CLI flags, updater behavior, installer identity, or supported platform contract.

After `1.0.0`:

- Patch bump: backward-compatible fixes.
- Minor bump: backward-compatible features.
- Major bump: breaking changes.

## Pre-Releases

Use pre-release versions only for test channels or internal QA builds, for example `0.9.0-beta.1`.

Do not publish a pre-release as the stable updater target unless a separate update channel exists and has been tested.

## Canonical Files

The app version must match in all of these files:

- `package.json`
- `src-tauri/Cargo.toml`
- `src-tauri/Cargo.lock`
- `src-tauri/tauri.conf.json`

Run this before building or releasing:

```bash
bun run check:version
```

For a tag-specific release check:

```bash
bun run check:version --tag=v0.8.7
```

## Release Checklist

1. Decide the bump level from the rules above.
2. Update all canonical version files.
3. Run `bun run check:version`.
4. Run the smallest relevant tests for the change, then broader build checks when producing installers.
5. Tag releases as `vMAJOR.MINOR.PATCH`.
6. Publish updater artifacts only from a clean, green release workflow.
