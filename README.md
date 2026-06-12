<div align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="src/assets/verbatim-lockup-horizontal-on-dark.svg">
    <img src="src/assets/verbatim-lockup-horizontal.svg" alt="Verbatim" width="400">
  </picture>

**A free, open source, and extensible speech-to-text application that works completely offline.**

[![Latest release](https://img.shields.io/github/v/release/GalaxyRuler/Verbatim?sort=semver&style=flat-square&logo=github)](https://github.com/GalaxyRuler/Verbatim/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/GalaxyRuler/Verbatim/total?style=flat-square&logo=github)](https://github.com/GalaxyRuler/Verbatim/releases)
[![License](https://img.shields.io/github/license/GalaxyRuler/Verbatim?style=flat-square)](https://github.com/GalaxyRuler/Verbatim/blob/main/LICENSE)
[![Platforms](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-2563eb?style=flat-square)](#download-verbatim)
[![Offline](https://img.shields.io/badge/privacy-local%20first-15803d?style=flat-square)](#why-verbatim)
[![Updater](https://img.shields.io/badge/updates-in--app-0f766e?style=flat-square)](#download-verbatim)

</div>

Verbatim is a cross-platform desktop application that provides simple, privacy-focused speech transcription. Press a shortcut, speak, and have your words appear in any text field. This happens on your own computer without sending any information to the cloud.

Verbatim is a fork of [Handy](https://github.com/cjpais/Handy) by [CJ Pais](https://github.com/cjpais), carrying its open-source foundation forward under a new identity and direction. See [Acknowledgments](#acknowledgments).

## Why Verbatim?

Verbatim exists to be a truly open source, extensible speech-to-text tool:

- **Free**: Accessibility tooling belongs in everyone's hands, not behind a paywall
- **Open Source**: Together we can build further. Extend Verbatim for yourself and contribute to something bigger
- **Private**: Your voice stays on your computer. Get transcriptions without sending audio to the cloud
- **Simple**: One tool, one job. Transcribe what you say and put it into a text box

Verbatim isn't trying to be the best speech-to-text app—it's trying to be the most forkable one.

## How It Works

1. **Press** a configurable keyboard shortcut to start/stop recording (or use push-to-talk mode)
2. **Speak** your words while the shortcut is active
3. **Release** and Verbatim processes your speech using Whisper
4. **Get** your transcribed text pasted directly into whatever app you're using

The process is entirely local:

- Silence is filtered using VAD (Voice Activity Detection) with Silero
- Transcription uses your choice of models:
  - **Whisper models** (Small/Medium/Turbo/Large) with GPU acceleration when available
  - **Parakeet V3** - CPU-optimized model with excellent performance and automatic language detection
- Works on Windows, macOS, and Linux

## Download Verbatim

Download the latest Windows, macOS, and Linux installers from the [Verbatim Releases page](https://github.com/GalaxyRuler/Verbatim/releases/latest).

<!-- latest-release:start -->

**Latest published release:** [v0.8.4](https://github.com/GalaxyRuler/Verbatim/releases/tag/v0.8.4) (2026-06-12)

| Platform             | Direct downloads                                                                                                                                                                                                      |
| -------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Windows x64          | [setup.exe](https://github.com/GalaxyRuler/Verbatim/releases/download/v0.8.4/Verbatim_0.8.4_x64-setup.exe) · [MSI](https://github.com/GalaxyRuler/Verbatim/releases/download/v0.8.4/Verbatim_0.8.4_x64_en-US.msi)     |
| Windows ARM64        | [setup.exe](https://github.com/GalaxyRuler/Verbatim/releases/download/v0.8.4/Verbatim_0.8.4_arm64-setup.exe) · [MSI](https://github.com/GalaxyRuler/Verbatim/releases/download/v0.8.4/Verbatim_0.8.4_arm64_en-US.msi) |
| macOS Apple Silicon  | [DMG](https://github.com/GalaxyRuler/Verbatim/releases/download/v0.8.4/Verbatim_0.8.4_aarch64.dmg)                                                                                                                    |
| macOS Intel          | [DMG](https://github.com/GalaxyRuler/Verbatim/releases/download/v0.8.4/Verbatim_0.8.4_x64.dmg)                                                                                                                        |
| Ubuntu/Debian x64    | [DEB](https://github.com/GalaxyRuler/Verbatim/releases/download/v0.8.4/Verbatim_0.8.4_amd64.deb)                                                                                                                      |
| Ubuntu/Debian ARM64  | [DEB](https://github.com/GalaxyRuler/Verbatim/releases/download/v0.8.4/Verbatim_0.8.4_arm64.deb)                                                                                                                      |
| Linux AppImage       | [x64](https://github.com/GalaxyRuler/Verbatim/releases/download/v0.8.4/Verbatim_0.8.4_amd64.AppImage) · [ARM64](https://github.com/GalaxyRuler/Verbatim/releases/download/v0.8.4/Verbatim_0.8.4_aarch64.AppImage)     |
| Fedora/RHEL/openSUSE | [x64 RPM](https://github.com/GalaxyRuler/Verbatim/releases/download/v0.8.4/Verbatim-0.8.4-1.x86_64.rpm) · [ARM64 RPM](https://github.com/GalaxyRuler/Verbatim/releases/download/v0.8.4/Verbatim-0.8.4-1.aarch64.rpm)  |

<!-- latest-release:end -->

Choose the asset that matches your operating system and CPU:

| Platform             | Recommended asset            | Notes                                                                               |
| -------------------- | ---------------------------- | ----------------------------------------------------------------------------------- |
| Windows x64          | `Verbatim_*_x64-setup.exe`   | Use the `.msi` for managed installs.                                                |
| Windows ARM64        | `Verbatim_*_arm64-setup.exe` | For Snapdragon/ARM Windows devices.                                                 |
| macOS Apple Silicon  | `Verbatim_*_aarch64.dmg`     | For M-series Macs.                                                                  |
| macOS Intel          | `Verbatim_*_x64.dmg`         | For Intel Macs.                                                                     |
| Ubuntu/Debian x64    | `Verbatim_*_amd64.deb`       | Installs recommended Linux helper packages through `apt` by default.                |
| Ubuntu/Debian ARM64  | `Verbatim_*_arm64.deb`       | For ARM64 Linux devices.                                                            |
| Fedora/RHEL/openSUSE | `Verbatim-*-*.rpm`           | Install distro helper packages if your package manager does not install recommends. |
| Other Linux distros  | `Verbatim_*_*.AppImage`      | Portable option; see [Linux Notes](#linux-notes) for helper tools.                  |

Installed builds can update in place from the footer update control when update checks are enabled. Portable installs may require a manual download from the Releases page.

## Quick Start

### Installation

**Recommended**

- [Download the latest Verbatim release](https://github.com/GalaxyRuler/Verbatim/releases/latest)

1. Install the application
2. Launch Verbatim and grant necessary system permissions (microphone, accessibility)
3. Configure your preferred keyboard shortcuts in Settings
4. Start transcribing!

### Development Setup

For detailed build instructions including platform-specific requirements, see [BUILD.md](docs/BUILD.md).

## Architecture

Verbatim is built as a Tauri application combining:

- **Frontend**: React + TypeScript with Tailwind CSS for the settings UI
- **Backend**: Rust for system integration, audio processing, and ML inference
- **Core Libraries**:
  - `whisper-rs`: Local speech recognition with Whisper models
  - `transcribe-rs`: CPU-optimized speech recognition with Parakeet models
  - `cpal`: Cross-platform audio I/O
  - `vad-rs`: Voice Activity Detection
  - `rdev`: Global keyboard shortcuts and system events
  - `rubato`: Audio resampling

### Debug Mode

Verbatim includes an advanced debug mode for development and troubleshooting. Access it by pressing:

- **macOS**: `Cmd+Shift+D`
- **Windows/Linux**: `Ctrl+Shift+D`

### CLI Parameters

Verbatim supports command-line flags for controlling a running instance and customizing startup behavior. These work on all platforms (macOS, Windows, Linux).

**Remote control flags** (sent to an already-running instance via the single-instance plugin):

```bash
verbatim --toggle-transcription    # Toggle recording on/off
verbatim --toggle-post-process     # Toggle recording with post-processing on/off
verbatim --cancel                  # Cancel the current operation
```

**Startup flags:**

```bash
verbatim --start-hidden            # Start without showing the main window
verbatim --no-tray                 # Start without the system tray icon
verbatim --debug                   # Enable debug mode with verbose logging
verbatim --help                    # Show all available flags
```

Flags can be combined for autostart scenarios:

```bash
verbatim --start-hidden --no-tray
```

> **macOS tip:** When Verbatim is installed as an app bundle, invoke the binary directly:
>
> ```bash
> /Applications/Verbatim.app/Contents/MacOS/Verbatim --toggle-transcription
> ```

## Known Issues & Current Limitations

This project is actively being developed and has some [known issues](https://github.com/GalaxyRuler/Verbatim/issues). We believe in transparency about the current state:

### Major Issues (Help Wanted)

**Whisper Model Crashes:**

- Whisper models crash on certain system configurations (Windows and Linux)
- Does not affect all systems - issue is configuration-dependent
  - If you experience crashes and are a developer, please help to fix and provide debug logs!

**Wayland Support (Linux):**

- Wayland does not allow one universal global-keyboard and text-injection API across all compositors
- Verbatim supports the common helper-tool path: `wtype` on wlroots compositors, `kwtype` on KDE when available, and `dotool`/`ydotool` as fallback options
- On Wayland, configure global shortcuts in your desktop environment or window manager and point them at Verbatim's CLI flags

### Linux Notes

**Text Input Tools:**

For reliable text input on Linux, install the appropriate helper for your display server. The `.deb` package recommends the common helpers so normal Ubuntu/Debian installs pull them in automatically. AppImage, RPM, Arch, and minimal installs may still need manual setup.

| Environment                        | Recommended tool                                  | Ubuntu/Debian                                    | Fedora/RHEL                                      | Arch                            |
| ---------------------------------- | ------------------------------------------------- | ------------------------------------------------ | ------------------------------------------------ | ------------------------------- |
| X11                                | `xdotool`                                         | `sudo apt install xdotool`                       | `sudo dnf install xdotool`                       | `sudo pacman -S xdotool`        |
| GNOME/Wayland, wlroots compositors | `wtype`                                           | `sudo apt install wtype`                         | `sudo dnf install wtype`                         | `sudo pacman -S wtype`          |
| KDE/Wayland                        | `kwtype` when packaged, otherwise clipboard paste | Install from your distro or KDE packaging source | Install from your distro or KDE packaging source | Install from your distro or AUR |
| Both                               | `dotool` or `ydotool`                             | Install from your distro if packaged             | Install from your distro if packaged             | `sudo pacman -S dotool ydotool` |
| Wayland clipboard                  | `wl-clipboard`                                    | `sudo apt install wl-clipboard`                  | `sudo dnf install wl-clipboard`                  | `sudo pacman -S wl-clipboard`   |

- **X11**: Install `xdotool` for both direct typing and clipboard paste shortcuts
- **Wayland**: Prefer clipboard-based paste with `wl-clipboard`; use `wtype` for direct typing on supported compositors
- **KDE Wayland**: `wtype` usually does not work because KDE does not expose the wlroots virtual keyboard protocol. Use clipboard paste, `kwtype` if available, or a compositor shortcut that calls `verbatim --toggle-transcription`.
- **dotool/ydotool setup**: These uinput-based tools may require a daemon, udev rules, or adding your user to the `input` group: `sudo usermod -aG input $USER` (then log out and back in)

Without these tools, Verbatim falls back to enigo which may have limited compatibility, especially on Wayland.

**Custom Dictionary Auto-Add:**

The custom dictionary auto-add watcher uses each platform's accessibility text APIs to read the focused text field after Verbatim pastes. Windows uses UI Automation, macOS uses the Accessibility permission already requested during onboarding, and Linux uses AT-SPI through `pyatspi`.

The `.deb` package recommends the Linux accessibility binding. For AppImage, RPM, or minimal installs, install it manually:

```bash
sudo apt install python3-pyatspi
```

Current limitation: this feature depends on the target app exposing focused text through the platform accessibility APIs. It may not learn edits from applications that hide text fields from Accessibility/AT-SPI, browser sandboxes, terminals, games, or remote-desktop sessions.

**Other Notes:**

- **Runtime library dependency (`libgtk-layer-shell.so.0`)**:
  - Verbatim links `gtk-layer-shell` on Linux. The `.deb` and `.rpm` packages declare this as a runtime dependency. If AppImage startup fails with `error while loading shared libraries: libgtk-layer-shell.so.0`, or if your package manager skipped dependencies, install the runtime package for your distro:

    | Distro        | Package to install    | Example command                        |
    | ------------- | --------------------- | -------------------------------------- |
    | Ubuntu/Debian | `libgtk-layer-shell0` | `sudo apt install libgtk-layer-shell0` |
    | Fedora/RHEL   | `gtk-layer-shell`     | `sudo dnf install gtk-layer-shell`     |
    | Arch Linux    | `gtk-layer-shell`     | `sudo pacman -S gtk-layer-shell`       |

  - For building from source on Ubuntu/Debian, you may also need `libgtk-layer-shell-dev`.

- The recording overlay is disabled by default on Linux (`Overlay Position: None`) because certain compositors treat it as the active window. When the overlay is visible it can steal focus, which prevents Verbatim from pasting back into the application that triggered transcription. If you enable the overlay anyway, be aware that clipboard-based pasting might fail or end up in the wrong window.
- If you are having trouble with the app, running with the environment variable `WEBKIT_DISABLE_DMABUF_RENDERER=1` may help
- If Verbatim fails to start reliably on Linux, see [Troubleshooting → Linux Startup Crashes or Instability](#linux-startup-crashes-or-instability).
- **Global keyboard shortcuts (Wayland):** On Wayland, system-level shortcuts must be configured through your desktop environment or window manager. Use the [CLI flags](#cli-parameters) as the command for your custom shortcut.

  **GNOME:**
  1. Open **Settings > Keyboard > Keyboard Shortcuts > Custom Shortcuts**
  2. Click the **+** button to add a new shortcut
  3. Set the **Name** to `Toggle Verbatim Transcription`
  4. Set the **Command** to `verbatim --toggle-transcription`
  5. Click **Set Shortcut** and press your desired key combination (e.g., `Super+O`)

  **KDE Plasma:**
  1. Open **System Settings > Shortcuts > Custom Shortcuts**
  2. Click **Edit > New > Global Shortcut > Command/URL**
  3. Name it `Toggle Verbatim Transcription`
  4. In the **Trigger** tab, set your desired key combination
  5. In the **Action** tab, set the command to `verbatim --toggle-transcription`

  **Sway / i3:**

  Add to your config file (`~/.config/sway/config` or `~/.config/i3/config`):

  ```ini
  bindsym $mod+o exec verbatim --toggle-transcription
  ```

  **Hyprland:**

  Add to your config file (`~/.config/hypr/hyprland.conf`):

  ```ini
  bind = $mainMod, O, exec, verbatim --toggle-transcription
  ```

- You can also manage global shortcuts outside of Verbatim via Unix signals, which lets Wayland window managers or other hotkey daemons keep ownership of keybindings:

  | Signal    | Action                                    | Example                   |
  | --------- | ----------------------------------------- | ------------------------- |
  | `SIGUSR2` | Toggle transcription                      | `pkill -USR2 -n verbatim` |
  | `SIGUSR1` | Toggle transcription with post-processing | `pkill -USR1 -n verbatim` |

  Example Sway config:

  ```ini
  bindsym $mod+o exec pkill -USR2 -n verbatim
  bindsym $mod+p exec pkill -USR1 -n verbatim
  ```

  `pkill` here simply delivers the signal—it does not terminate the process.

**Overlay & Pasting Issues (Linux):**

- The recording overlay window can interfere with pasting transcribed text into target applications on Linux (X11)
- **Solution:** Open **Settings > Advanced** and set **"Overlay Position"** to **"None"** to disable the overlay
- Enable **"Audio Feedback"** (also in Advanced) if you still want audible confirmation of recording state
- Users who upgrade from older versions or import settings from other platforms may need to manually apply this change

### Platform Support

- **macOS (both Intel and Apple Silicon)**
- **Windows (x64 and ARM64)**
- **Linux (x64 and ARM64 packages; AppImage, Debian, and RPM formats)**

### System Requirements/Recommendations

The following are recommendations for running Verbatim on your own machine. If you don't meet the system requirements, the performance of the application may be degraded. We are working on improving the performance across all kinds of computers and hardware.

**For Whisper Models:**

- **macOS**: M series Mac, Intel Mac
- **Windows**: Intel, AMD, or NVIDIA GPU
- **Linux**: Intel, AMD, or NVIDIA GPU
  - Ubuntu 22.04, 24.04

**For Parakeet V3 Model:**

- **CPU-only operation** - runs on a wide variety of hardware
- **Minimum**: Intel Skylake (6th gen) or equivalent AMD processors
- **Performance**: ~5x real-time speed on mid-range hardware (tested on i5)
- **Automatic language detection** - no manual language selection required

## Roadmap & Active Development

We're actively working on several features and improvements. Contributions and feedback are welcome!

### In Progress

**Debug Logging:**

- Adding debug logging to a file to help diagnose issues

**macOS Keyboard Improvements:**

- Support for Globe key as transcription trigger
- A rewrite of global shortcut handling for MacOS, and potentially other OS's too.

**Opt-in Analytics:**

- Collect anonymous usage data to help improve Verbatim
- Privacy-first approach with clear opt-in

**Settings Refactoring:**

- Cleanup and refactor settings system which is becoming bloated and messy
- Implement better abstractions for settings management

**Tauri Commands Cleanup:**

- Abstract and organize Tauri command patterns
- Investigate tauri-specta for improved type safety and organization

## Verify Release Signatures

Current public installers are unsigned unless the release notes explicitly say otherwise. This means Windows SmartScreen and macOS Gatekeeper may warn before first launch.

When a release includes matching `.sig` files, Verbatim release artifacts can be verified with Tauri's updater signature format. The public key is stored in [`src-tauri/tauri.conf.json`](src-tauri/tauri.conf.json) under `plugins.updater.pubkey`.

To verify a signed release manually, set `ARTIFACT` to the filename you downloaded, save the `pubkey` value from `src-tauri/tauri.conf.json` to `verbatim.pub.b64`, then decode the public key and matching `.sig` file from base64 and verify the artifact with `minisign`:

```bash
# Replace with the file you downloaded
ARTIFACT="Verbatim_0.8.1_amd64.AppImage"

python3 - "$ARTIFACT" <<'PY'
import base64, pathlib, sys

artifact = sys.argv[1]

pub = pathlib.Path("verbatim.pub.b64").read_text().strip()
pathlib.Path("verbatim.pub").write_bytes(base64.b64decode(pub))

sig = pathlib.Path(f"{artifact}.sig").read_text().strip()
pathlib.Path(f"{artifact}.minisig").write_bytes(base64.b64decode(sig))
PY

minisign -Vm "$ARTIFACT" \
  -p verbatim.pub \
  -x "$ARTIFACT.minisig"
```

On success, `minisign` prints:

```text
Signature and comment signature verified
```

Do not use `gpg` for these `.sig` files.

## Troubleshooting

### Manual Model Installation (For Proxy Users or Network Restrictions)

If you're behind a proxy, firewall, or in a restricted network environment where Verbatim cannot download models automatically, you can manually download and install them. The URLs are publicly accessible from any browser.

#### Step 1: Find Your App Data Directory

1. Open Verbatim settings
2. Navigate to the **About** section
3. Copy the "App Data Directory" path shown there, or use the shortcuts:
   - **macOS**: `Cmd+Shift+D` to open debug menu
   - **Windows/Linux**: `Ctrl+Shift+D` to open debug menu

The typical paths are:

- **macOS**: `~/Library/Application Support/com.galaxyruler.verbatim/`
- **Windows**: `C:\Users\{username}\AppData\Roaming\com.galaxyruler.verbatim\`
- **Linux**: `~/.config/com.galaxyruler.verbatim/`

#### Step 2: Create Models Directory

Inside your app data directory, create a `models` folder if it doesn't already exist:

```bash
# macOS/Linux
mkdir -p ~/Library/Application\ Support/com.galaxyruler.verbatim/models

# Windows (PowerShell)
New-Item -ItemType Directory -Force -Path "$env:APPDATA\com.galaxyruler.verbatim\models"
```

#### Step 3: Download Model Files

Download the models you want from below

**Whisper Models (single .bin files):**

Downloaded from the official [ggerganov/whisper.cpp](https://huggingface.co/ggerganov/whisper.cpp) repository on Hugging Face where available:

- Small (487 MB): `https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin`
- Medium (492 MB): `https://verbatim-assets.galaxyruler.space/whisper-medium-q4_1.bin` (custom q4_1 quantization originally packaged by the upstream Handy project; not available on Hugging Face)
- Turbo (1600 MB): `https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin`
- Large (1100 MB): `https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-q5_0.bin`

**Parakeet Models (compressed archives):**

These are int8-quantized packagings (by the upstream Handy project) of NVIDIA's [parakeet-tdt-0.6b-v2](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v2) and [parakeet-tdt-0.6b-v3](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3) models from Hugging Face:

- V2 (473 MB): `https://verbatim-assets.galaxyruler.space/parakeet-v2-int8.tar.gz`
- V3 (478 MB): `https://verbatim-assets.galaxyruler.space/parakeet-v3-int8.tar.gz`

#### Step 4: Install Models

**For Whisper Models (.bin files):**

Simply place the `.bin` file directly into the `models` directory:

```
{app_data_dir}/models/
├── ggml-small.bin
├── whisper-medium-q4_1.bin
├── ggml-large-v3-turbo.bin
└── ggml-large-v3-q5_0.bin
```

**For Parakeet Models (.tar.gz archives):**

1. Extract the `.tar.gz` file
2. Place the **extracted directory** into the `models` folder
3. The directory must be named exactly as follows:
   - **Parakeet V2**: `parakeet-tdt-0.6b-v2-int8`
   - **Parakeet V3**: `parakeet-tdt-0.6b-v3-int8`

Final structure should look like:

```
{app_data_dir}/models/
├── parakeet-tdt-0.6b-v2-int8/     (directory with model files inside)
│   ├── (model files)
│   └── (config files)
└── parakeet-tdt-0.6b-v3-int8/     (directory with model files inside)
    ├── (model files)
    └── (config files)
```

**Important Notes:**

- For Parakeet models, the extracted directory name **must** match exactly as shown above
- Do not rename the `.bin` files for Whisper models—use the exact filenames from the download URLs
- After placing the files, restart Verbatim to detect the new models

#### Step 5: Verify Installation

1. Restart Verbatim
2. Open Settings → Models
3. Your manually installed models should now appear as "Downloaded"
4. Select the model you want to use and test transcription

### Custom Whisper Models

Verbatim can auto-discover custom Whisper GGML models placed in the `models` directory. This is useful for users who want to use fine-tuned or community models not included in the default model list.

**How to use:**

1. Obtain a Whisper model in GGML `.bin` format (e.g., from [Hugging Face](https://huggingface.co/models?search=whisper%20ggml))
2. Place the `.bin` file in your `models` directory (see paths above)
3. Restart Verbatim to discover the new model
4. The model will appear in the "Custom Models" section of the Models settings page

**Important:**

- Community models are user-provided and may not receive troubleshooting assistance
- The model must be a valid Whisper GGML format (`.bin` file)
- Model name is derived from the filename (e.g., `my-custom-model.bin` → "My Custom Model")

### Linux Startup Crashes or Instability

If Verbatim fails to start reliably on Linux — for example, it crashes shortly after launch, never shows its window, or reports a Wayland protocol error — try the steps below in order.

**1. Install (or reinstall) `gtk-layer-shell`**

Verbatim uses `gtk-layer-shell` for its recording overlay and links against it at runtime. A missing or broken installation is the most common cause of startup failures and can manifest as a crash or a hang well before any window is shown. Make sure the runtime package is installed for your distro:

| Distro        | Package to install    | Example command                        |
| ------------- | --------------------- | -------------------------------------- |
| Ubuntu/Debian | `libgtk-layer-shell0` | `sudo apt install libgtk-layer-shell0` |
| Fedora/RHEL   | `gtk-layer-shell`     | `sudo dnf install gtk-layer-shell`     |
| Arch Linux    | `gtk-layer-shell`     | `sudo pacman -S gtk-layer-shell`       |

If it is already installed and you still see startup problems, try reinstalling it (e.g. `sudo pacman -S gtk-layer-shell` again) in case the library files were corrupted by a partial upgrade.

**2. Disable the GTK layer shell overlay (`VERBATIM_NO_GTK_LAYER_SHELL`)**

If installing the library does not help, you can skip `gtk-layer-shell` initialization entirely as a workaround. On some compositors (notably KDE Plasma under Wayland) it has been reported to interact poorly with the recording overlay. With this variable set, the overlay falls back to a regular always-on-top window:

```bash
VERBATIM_NO_GTK_LAYER_SHELL=1 verbatim
```

**3. Disable WebKit DMA-BUF renderer (`WEBKIT_DISABLE_DMABUF_RENDERER`)**

On some GPU/driver combinations the WebKitGTK DMA-BUF renderer can cause the window to fail to render or to crash. Try:

```bash
WEBKIT_DISABLE_DMABUF_RENDERER=1 verbatim
```

**Making a workaround permanent**

Once you've found a flag that helps, export it from your shell profile (`~/.bashrc`, `~/.zshenv`, …) or from the desktop autostart entry that launches Verbatim. If you launch Verbatim from a `.desktop` file, you can prefix the `Exec=` line, e.g.:

```ini
Exec=env VERBATIM_NO_GTK_LAYER_SHELL=1 verbatim
```

If a workaround helps you, please [open an issue](https://github.com/GalaxyRuler/Verbatim/issues) describing your distro, desktop environment, and session type — that information helps us narrow down the underlying bug.

### How to Contribute

1. **Check existing issues** at [github.com/GalaxyRuler/Verbatim/issues](https://github.com/GalaxyRuler/Verbatim/issues)
2. **Fork the repository** and create a feature branch
3. **Test thoroughly** on your target platform
4. **Submit a pull request** with clear description of changes
5. **Join the discussion** - reach out in [GitHub Discussions](https://github.com/GalaxyRuler/Verbatim/discussions)

The goal is to create both a useful tool and a foundation for others to build upon—a well-patterned, simple codebase that serves the community.

## Contributor Attribution

Verbatim keeps the upstream Handy Git history and credits intact for transparency and license compliance. Public contributor lists, badges, and future contributor files should distinguish between post-fork Verbatim contributions and upstream Handy work.

Upstream Handy contributors are acknowledged in [Acknowledgments](#acknowledgments). Verbatim contributor recognition is reserved for work contributed to this repository after the Verbatim fork.

## Related Projects

- **[Handy](https://github.com/cjpais/Handy)** - The original project Verbatim is forked from, by CJ Pais
- **[Handy CLI](https://github.com/cjpais/handy-cli)** - The original Python command-line version of Handy
- **[handy.computer](https://handy.computer)** - The upstream Handy project's website with demos and documentation

## License

MIT License - see [LICENSE](LICENSE) file for details. Verbatim retains the original copyright notice of the upstream Handy project, as the license requires.

## Acknowledgments

Verbatim stands on the shoulders of the people who built its foundations:

- **[Handy](https://github.com/cjpais/Handy)** by **[CJ Pais](https://github.com/cjpais)** and its contributors and sponsors — the original application this project is forked from. Their work on the architecture, audio pipeline, and cross-platform support made Verbatim possible.
- **Whisper** by OpenAI for the speech recognition model
- **whisper.cpp and ggml** by Georgi Gerganov and contributors for amazing cross-platform whisper inference/acceleration ([Hugging Face repository](https://huggingface.co/ggerganov/whisper.cpp))
- **Parakeet** by NVIDIA for the CPU-friendly transcription models ([V2](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v2), [V3](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3) on Hugging Face)
- **Silero** for great lightweight VAD
- **Tauri** team for the excellent Rust-based app framework
- **Verbatim community contributors** helping make this fork better after the Verbatim project started
