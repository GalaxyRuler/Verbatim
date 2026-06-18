# Android Icons

Android launcher icons are maintained from the master SVGs in
`src-tauri/icons/android/`:

- `verbatim-icon-launcher.svg` renders `ic_launcher.png`.
- `verbatim-icon-round.svg` renders `ic_launcher_round.png`.
- `verbatim-icon-foreground.svg` renders `ic_launcher_foreground.png` and is
  also the visual source for the monochrome/themed icon layer.

The square icon uses a filled dictation-bubble symbol with the first five pulse
bars from the Verbatim mark. The full Verbatim lockup is too wide for a square
launcher icon and should stay in surfaces that can support its aspect ratio.

Regenerate Android PNGs with:

```bash
bash scripts/regen-android-icons.sh
```

This renders all PNG launcher assets in both Android icon trees:

- `src-tauri/icons/android/`
- `src-tauri/gen/android/app/src/main/res/`

Android 12+ launch screens use
`src-tauri/gen/android/app/src/main/res/drawable/ic_launcher_splash.xml`. Keep
that drawable self-contained because the platform may render it on a plain
white splash background.

Keep the foreground master self-contained too. The generated Android/Tauri
launch path can render `ic_launcher_foreground` directly before the WebView is
ready, so a transparent foreground produces a broken blue-bars-on-white mark.
The monochrome/themed icon layer intentionally uses the bubble silhouette only;
fine waveform cutouts collapse into a cross-like shape in Pixel Launcher's
themed icon and splash rendering.
The adaptive wrappers in `mipmap-anydpi-v26` point at the generated
`@mipmap/ic_launcher_foreground` PNG rather than the vector foreground. Pixel
Launcher rendered the vector composition as a cross-like shape, while the PNG
matches the checked master art across densities.

Do not use `tauri icon` for Android icons. It rewrites the raster assets without
preserving the adaptive wrappers and monochrome/themed resources used for
Android launcher rendering, which can put the app back into an off-brand state.
