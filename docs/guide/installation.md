# Installation

Download the latest build from the [Releases page](https://github.com/GalaxyRuler/Verbatim/releases/latest), then follow the steps for your platform.

## Windows (x64)

Two formats are published:

- **Installer:** `Verbatim_<version>_x64-setup.exe` (recommended)
- **MSI:** `Verbatim_<version>_x64_en-US.msi`

Run the installer. Because the builds are currently **unsigned**, Windows SmartScreen may warn that the publisher is unrecognized. Choose **More info → Run anyway**. You only see this once.

## macOS (Apple Silicon)

- `Verbatim_<version>_aarch64.dmg`

Open the `.dmg` and drag **Verbatim** into Applications. On first launch, macOS Gatekeeper may block an unsigned app. **Right-click (or Control-click) the app → Open**, then confirm in the dialog. After the first time it opens normally.

> Intel (x86) Macs are not currently built. Check the Releases page for changes.

## Linux (Ubuntu / Debian)

- `Verbatim_<version>_amd64.deb`

Install it with:

```bash
sudo dpkg -i Verbatim_<version>_amd64.deb
sudo apt-get install -f   # pull in any missing dependencies
```

## Verifying your download

Each release publishes its binaries together with signature files (`.sig`) and an updater manifest. See the [Releases page](https://github.com/GalaxyRuler/Verbatim/releases/latest) for the full asset list.
