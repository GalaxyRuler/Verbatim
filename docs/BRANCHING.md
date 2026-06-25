# Branching & Platform Strategy

Verbatim is **one Tauri codebase** that targets desktop (Windows/macOS/Linux), Android, and iOS from the **same source** (`src/`, `src-tauri/`). This document is the rule for how we branch and how platforms are kept apart.

## TL;DR

- **One trunk: `main`.** It builds _all_ platforms. There is **no separate "desktop branch" and "Android branch".**
- **Platforms are separated by `#[cfg(...)]` + CI path filters, not by branches.**
- **Feature branches are short-lived** — rebase often, merge fast (days, not weeks).

## Why no per-platform long-lived branches

A permanent `android` (or `desktop`) branch would have to re-merge every change on the other branch forever. The codebase is shared, so two trunks = double the conflicts and constant drift. We already paid for this once: a feature branch that lagged `main` produced a multi-day merge cascade (test-helper 3-way merges, re-applied platform stubs, CI fallout). The fix is the opposite of splitting — **merge to `main` frequently** so nothing diverges.

## How platforms stay apart (without branches)

- **Code:** platform-specific code is gated, e.g. `#[cfg(target_os = "android")]` / `#[cfg(not(any(target_os = "android", target_os = "ios")))]`. Android stubs live beside desktop impls (e.g. `overlay.rs` vs `overlay_stub.rs`). The on-device ASR engine is `cfg(android)` in `src-tauri/src/asr/`.
- **CI:** each platform has its own workflow, scoped with `on: pull_request: paths:` so it only runs when relevant files change:
  - desktop: `native-backend.yml`, `native-smoke.yml`, the production-backend matrix
  - Android: `android.yml` (regen-guard + Maestro e2e + `check-android-so-alignment.ts`)
  - shared: `code-quality`, `playwright`, `rust-tests`, `nix-build`

## Workflow

1. **Branch off `main`** with a scoped name: `android-asr-g1`, `fix/native-packaging-ci`, `docs/...`.
2. **Keep it short-lived.** Rebase onto `main` whenever `main` moves; don't let it age.
3. **One PR → review/audit → merge.** Large multi-step features ship as **stacked small PRs** (e.g. Android ASR: G0 → G1 → G2…), each merged as it passes, not one giant branch.
4. **Delete the branch after merge.**

## Required checks (branch protection)

- `main` requires **`code-quality`** (and may add **`android`** once the e2e lane is stable — symmetric to the desktop lanes).
- **Do not mark a path-filtered check as required.** A required check that is skipped by its `paths:` filter sits "Expected — waiting" and deadlocks the PR forever. To require a platform lane, first remove its path filter so it runs on every PR.
- Non-required lanes (e.g. packaged-smoke) may be red without blocking merge; fix them in their own PRs.

## When a separate branch _is_ allowed

- Short-lived feature / spike / docs branches (the norm).
- A `release/x.y` branch **only** during release stabilization, deleted afterward.
- Never a permanent platform fork.
