# Android Icons

Android launcher icons are maintained from the master SVGs in
`src-tauri/icons/android/`:

- `verbatim-icon-launcher.svg` renders `ic_launcher.png`.
- `verbatim-icon-round.svg` renders `ic_launcher_round.png`.
- `verbatim-icon-foreground.svg` renders `ic_launcher_foreground.png` and is
  also the visual source for the monochrome/themed icon layer.

The square icon uses a dictation-bubble symbol with the first five pulse bars
from the Verbatim mark. The full Verbatim lockup is too wide for a square
launcher icon and should stay in surfaces that can support its aspect ratio.

Regenerate Android PNGs with:

```bash
bash scripts/regen-android-icons.sh
```

This renders all PNG launcher assets in both Android icon trees:

- `src-tauri/icons/android/`
- `src-tauri/gen/android/app/src/main/res/`

Do not use `tauri icon` for Android icons. It rewrites the raster assets without
preserving the adaptive vector foreground/monochrome resources used for Android
themed icons, which can put the app back into an off-brand state.
