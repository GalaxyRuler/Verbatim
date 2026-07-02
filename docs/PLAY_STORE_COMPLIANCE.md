# Google Play Store Compliance

Last updated: 2026-07-02

This document tracks everything required for the Verbatim Android app
(`com.galaxyruler.verbatim`) to comply with the Google Play Developer Programme
Policies, and provides ready-to-paste answers for the Play Console declaration
forms.

## Status overview

| Requirement                                          | Where it is handled                                                                                           | Status        |
| ---------------------------------------------------- | ------------------------------------------------------------------------------------------------------------- | ------------- |
| Accessibility service marked as accessibility tool   | `android:isAccessibilityTool="true"` in `verbatim_accessibility_service.xml`                                  | Done (in app) |
| Prominent disclosure before enabling accessibility   | Disclosure sheet in Android onboarding (`AndroidApp.tsx`), shown before opening system Accessibility settings | Done (in app) |
| Accessibility settings description explains data use | `verbatim_accessibility_description` in `strings.xml`                                                         | Done (in app) |
| Privacy policy URL                                   | <https://verbatim.alkulaib.io/privacy> (page lives in the `mysiteagain` repo)                                 | Page required |
| Foreground service (microphone) declaration          | Play Console → App content → Foreground service permissions                                                   | Console task  |
| Data safety form                                     | Play Console → App content → Data safety                                                                      | Console task  |
| Play App Signing with existing key                   | Play Console first-upload flow + PEPK export                                                                  | Console task  |
| Closed-testing requirement (personal accounts)       | Play Console testing tracks                                                                                   | Conditional   |

## In-app implementation (this repo)

- `src-tauri/gen/android/app/src/main/res/xml/verbatim_accessibility_service.xml`
  declares `android:isAccessibilityTool="true"`. Verbatim's dictation and
  text-insertion flow is an input aid usable by people who cannot or prefer not
  to type, which is the basis for the accessibility-tool positioning.
- The Android onboarding shows a prominent disclosure sheet before sending the
  user to system Accessibility settings. It states what the service does
  (inserts dictated text into the focused field), what it accesses (the focused
  text field only, skipping password/sensitive fields), and what it never does
  (collect, store, or share screen content). The user must tap
  "Agree and continue" before the settings screen opens.
- The service itself only resolves the input-focused node
  (`findFocus(FOCUS_INPUT)`) and refuses password fields (`isPassword`). It
  does not scrape or log window content.

## Play Console: Accessibility API declaration

If Play Console asks for a declaration of AccessibilityService API usage
(shown when the app is not treated as an accessibility tool, or during review),
use:

> Verbatim is a voice-dictation tool that lets users type with their voice in
> any app. The AccessibilityService API is used for exactly one purpose:
> inserting the user's dictated, on-device-transcribed text into the currently
> focused text field, at the cursor, when the user taps the floating Verbatim
> bubble. The service reads only the input-focused node so it can place text;
> it explicitly skips password and other sensitive fields. It does not read,
> collect, store, or transmit screen content, and no accessibility data leaves
> the device. This is core functionality: without it the app cannot deliver
> dictated text into other apps. A prominent in-app disclosure describing this
> usage is shown, and affirmative consent is required, before the user is taken
> to the system Accessibility settings to enable the service.

## Play Console: Foreground service (microphone) declaration

Target SDK 34+ requires declaring each foreground service type. Verbatim uses
`FOREGROUND_SERVICE_MICROPHONE` for `FloatingBubbleService`.

Declaration text:

> Verbatim is a voice-dictation app. When the user taps the floating dictation
> bubble, the app records microphone audio in a foreground service and
> transcribes it on-device. The foreground service is required because
> dictation happens while the user is in another app (the bubble floats over
> the target app), so recording must continue while Verbatim is not the
> foreground activity. A persistent notification is shown for the duration of
> recording. Recording starts and stops only on explicit user action.

Demo video requirements (record one, ~30 s, upload as unlisted YouTube link):

1. Show the app's onboarding/permission state briefly.
2. Open another app (e.g., a notes app), tap the floating bubble.
3. Show the microphone foreground-service notification in the shade while
   dictating.
4. Speak a sentence; show the text being inserted into the notes app.
5. Tap the bubble to stop; show the notification disappearing.

## Play Console: Data safety form

Base truth (see `docs/PRIVACY.md` for the full data-flow audit):

- Speech transcription runs on-device. Audio never leaves the device.
- Transcripts and recordings are stored locally only.
- No analytics or telemetry.
- Network traffic: update checks (GitHub), model downloads (GitHub /
  Hugging Face / Verbatim asset host), and — only if the user enables it and
  configures a provider — remote LLM post-processing, which sends transcript
  text to the user-chosen provider.

Suggested answers:

| Question                                                              | Answer                                                                                                                              |
| --------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| Does your app collect or share any of the required user data types?   | **Yes** (solely because optional remote post-processing can transmit transcript text off-device)                                    |
| Is all of the user data collected by your app encrypted in transit?   | **Yes** (HTTPS)                                                                                                                     |
| Do you provide a way for users to request that their data is deleted? | **Yes** — all data is local; users delete history/recordings in-app or by clearing app data. No server-side account or data exists. |

Data types:

- **Audio: Voice or sound recordings** — _Not collected._ Processed ephemerally
  on-device; recordings that are kept never leave the device. (Play's
  "collection" definition covers off-device transmission; on-device-only
  processing and storage does not count.)
- **Other user-generated content (transcript text)** — _Collected, optional._
  Only when the user enables AI post-processing with a remote provider.
  Purpose: App functionality. Ephemeral processing: Yes. Shared: No (sent to a
  service endpoint the user configures, acting on the user's instruction; not
  shared by the developer for advertising or analytics).
- Everything else (location, contacts, identifiers, etc.) — _Not collected._

If you later decide to remove remote post-processing from the Android build
entirely, the whole form collapses to "No data collected or shared."

## Play Console: other App content sections

| Section                         | Answer                                                                                                                                                                                                                                                                 |
| ------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Privacy policy                  | `https://verbatim.alkulaib.io/privacy`                                                                                                                                                                                                                                 |
| Ads                             | No ads                                                                                                                                                                                                                                                                 |
| App access                      | All functionality available without login; no credentials needed. If review needs the accessibility flow demonstrated, add instructions: "Complete onboarding: grant mic, overlay, accessibility; download offline speech pack; then dictate via the floating bubble." |
| Content rating questionnaire    | Utility/productivity app; no user-generated public content, no violence, etc. Expected rating: Everyone / PEGI 3                                                                                                                                                       |
| Target audience                 | 18+ (or 13+; do not target children — avoids Families policy requirements)                                                                                                                                                                                             |
| News app                        | No                                                                                                                                                                                                                                                                     |
| COVID-19 contact tracing/status | No                                                                                                                                                                                                                                                                     |
| Data safety                     | See above                                                                                                                                                                                                                                                              |
| Government app                  | No                                                                                                                                                                                                                                                                     |
| Financial features              | None                                                                                                                                                                                                                                                                   |
| Health apps                     | Not a health app                                                                                                                                                                                                                                                       |

## First upload / signing

- Upload the **AAB** (`Verbatim_x.y.z_android_universal.aab`), not the APK.
- Enroll in **Play App Signing** choosing **"Use existing key"** and export the
  current upload/signing key with Google's PEPK tool, so the Play-delivered
  signature matches the sideloaded releases (sideload users can then update
  from Play without reinstalling). See `docs/SIGNING.md` for keystore details.
- ABIs shipped: `arm64-v8a`, `x86_64` (32-bit ABIs dropped for the 16 KB
  page-alignment requirement). Play's 16 KB native-alignment requirement for
  new targetSdk 35+ submissions is satisfied — CI enforces it.

## Account-level gates

- Personal developer accounts created after November 2023 must run a **closed
  test with at least 12 testers continuously for 14 days** before production
  access can be requested. Plan the release timeline around this if it applies.
- App name "Verbatim" collides with the Verbatim storage-media trademark
  (different goods class, so likely acceptable, but an IP complaint is
  possible). Keep branding, iconography, and store copy clearly distinct from
  the storage brand.

## Store listing privacy claims

Keep the store listing consistent with `docs/PRIVACY.md`: transcription is
on-device; update checks and model downloads use the network; optional remote
post-processing sends transcript text to the provider the user configures. Do
not claim "no network activity."
