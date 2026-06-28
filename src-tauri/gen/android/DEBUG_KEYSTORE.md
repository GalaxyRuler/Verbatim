# Android Debug Keystore

`debug.keystore` is a checked-in, throwaway Android debug signing key used only
for debug APKs and CI emulator installs. It keeps `com.galaxyruler.verbatim`
debug signatures stable across builds so cached AVDs can update or reinstall
the app reliably.

Never use this key for release signing.

The credentials are the Android debug defaults:

- store password: `android`
- key alias: `androiddebugkey`
- key password: `android`
