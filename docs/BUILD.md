# Build Instructions

This guide covers how to set up the development environment and build Verbatim from source across different platforms.

## Prerequisites

### All Platforms

- [Rust](https://rustup.rs/) (latest stable)
- [Bun](https://bun.sh/) package manager
- [Tauri Prerequisites](https://tauri.app/start/prerequisites/)

### Platform-Specific Requirements

#### macOS

- Xcode Command Line Tools
- Install with: `xcode-select --install`

#### Windows

- Microsoft C++ Build Tools
- Visual Studio 2019/2022 with C++ development tools
- Or Visual Studio Build Tools 2019/2022
- Ninja on `PATH`
- Vulkan SDK with `VULKAN_SDK` set

The Windows Whisper/Vulkan dependency generates very deep CMake paths. Use the
repo scripts for backend checks and tests so Cargo builds in a short target
directory and CMake uses Ninja:

```powershell
bun run check:backend:windows
bun run test:backend:windows -- portable --lib
bun run build:windows:installer
```

To run Cargo directly from PowerShell, use the same environment:

```powershell
$env:CARGO_TARGET_DIR = "C:\t\verbatim"
$env:CMAKE_GENERATOR = "Ninja"
$env:TrackFileAccess = "false"
cargo test --manifest-path src-tauri\Cargo.toml portable --lib
```

#### Linux

- Build essentials
- ALSA development libraries
- Install with:

  ```bash
  # Ubuntu
  sudo apt update
  sudo apt install build-essential libasound2-dev pkg-config libssl-dev libvulkan-dev vulkan-tools glslc libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libgtk-layer-shell0 libgtk-layer-shell-dev patchelf cmake
  ```

## Setup Instructions

### 1. Clone the Repository

```bash
git clone git@github.com:GalaxyRuler/Verbatim.git
cd Verbatim
```

### 2. Install Dependencies

```bash
bun install
```

### 3. Start Dev Server

```bash
bun tauri dev
```

### 4. Build for Production

```bash
bun run tauri build
```

This compiles a release binary and generates platform-specific bundles for the supported release targets: `.deb` on Ubuntu x64, `.dmg` on macOS Apple Silicon, and Windows installers on Windows x64.

On Windows, use the wrapper instead of plain `bun run tauri build`:

```powershell
bun run build:windows:installer
```

The wrapper sets a short `CARGO_TARGET_DIR` before Tauri invokes Cargo. This
avoids MSBuild `FileTracker` failures in the generated Whisper/Vulkan shader
helper when CMake creates paths near Windows' legacy path limit.

## Generated Bindings

`src/bindings.ts` is generated from Rust commands, events, and exported types.
Regenerate it after adding, removing, or changing backend commands or exported
Rust types.

Use the dedicated no-default-features generator for routine binding updates:

```bash
bun run bindings:generate
```

This compiles the command/type registry without the native transcription engine
feature, so binding regeneration is not blocked by Whisper/ONNX CMake or GPU
toolchain failures.

Use a debug Tauri build only when you also need to validate debug app startup or
bundling behavior:

```bash
bun run tauri build -- --debug
```

On Windows, use the repository wrapper or the short-target environment described
above if a broader native build hits path-length or CMake generator failures.

## Backend Test Modes

Backend checks run in two useful modes:

- `--no-default-features` compiles the backend without the native transcription
  engine. Use this for settings, insertion, clipboard, history, cancellation,
  model-catalog, and command logic that does not need real Whisper/ONNX runtime
  linkage.
- Default features compile the real transcription engine stack. Use this before
  changing speech inference, model loading, audio-to-text behavior, accelerator
  selection, or any code whose correctness depends on the native engine.

Examples:

```bash
cargo test --manifest-path src-tauri/Cargo.toml operation_cancellation --lib --no-default-features --no-run
cargo check --manifest-path src-tauri/Cargo.toml --no-default-features
cargo test --manifest-path src-tauri/Cargo.toml model_catalog --lib --no-run
```

On Windows, prefer the repository wrapper scripts for broad backend checks
because they set a short target directory and CMake/Ninja environment. If a
default-feature test fails before crate compilation in a native dependency,
report that as a native build environment failure rather than as proof that the
Rust unit under test failed.

## Linux Install (from source)

The raw binary (`src-tauri/target/release/verbatim`) cannot run standalone — it needs Tauri resource files (tray icons, sounds, VAD model) to be co-located at the expected path.

**Install from the deb bundle** (supported on Ubuntu x64):

```bash
cd /tmp
ar x /path/to/Verbatim/src-tauri/target/release/bundle/deb/Verbatim_*_amd64.deb data.tar.gz
tar xzf data.tar.gz
sudo cp usr/bin/verbatim /usr/bin/
sudo cp -r usr/lib/Verbatim /usr/lib/
sudo cp -r usr/share/icons/hicolor/* /usr/share/icons/hicolor/
sudo cp usr/share/applications/Verbatim.desktop /usr/share/applications/
```

After subsequent rebuilds, only the binary needs re-copying:

```bash
sudo cp src-tauri/target/release/verbatim /usr/bin/
```

Resources only need re-copying if they change upstream (new icons, sounds, etc.).

## Unsupported Platforms

Intel Mac, Windows ARM64, Linux ARM64, AppImage, and RPM/Fedora-style packages are not official release targets right now. The source may still build elsewhere, but CI and releases only cover Windows x64, macOS Apple Silicon, and Ubuntu x64.
