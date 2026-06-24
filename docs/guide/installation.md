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

## Android

- `Verbatim_<version>_android_universal.apk` for direct installation
- `Verbatim_<version>_android_universal.aab` for app-bundle distribution

Install the APK from the release page on your Android device. Android may ask you to allow installs from that source before the package can be installed.

The AAB is not directly installable on a phone. Use it only for app-bundle distribution tooling.

## Verifying your download

Desktop releases publish updater/signature assets where available. Android releases publish signed APK/AAB assets and `SHA256SUMS.txt` checksums. See the [Releases page](https://github.com/GalaxyRuler/Verbatim/releases/latest) for the full asset list.
