# Verbatim design system

This is the contract for every UI change in Verbatim. If a change conflicts
with this document, either the change is wrong or this document must be
updated in the same PR — never silently diverge.

## Identity

Verbatim is a precise instrument, not a dashboard. The interface exists to
get speech into text and stay out of the way. Nothing exists for decoration
alone: every color, icon, and motion either communicates state or is removed.

## Tokens

`src/App.css` `@theme` is the **only** source of design tokens.
`tailwind.config.js` has been deleted; Tailwind v4 generates utilities
directly from the `@theme` block. Light values live in `@theme`, dark
overrides in the `prefers-color-scheme: dark` block of the same file.

| Group | Tokens |
| --- | --- |
| Core | `text`, `text-secondary`, `text-disabled`, `background`, `surface`, `border`, `border-strong` |
| Interaction | `accent`, `accent-fg` |
| Status | `success`, `success-bg`, `warning`, `warning-bg`, `danger`, `danger-bg`, `info`, `info-bg` |
| Shadow | `shadow-menu` (dropdown/tooltip panels) |
| Brand | `logo-primary`, `logo-stroke`, `text-stroke` — **logo rendering only**, never for UI surfaces or text |

Rules:

- **Raw Tailwind palette classes are forbidden** (`red-600`, `green-500`,
  `amber-600`, `gray-300`, ...). Use the semantic tokens above. Status tints
  come from the `*-bg` tokens (`bg-danger-bg`), not from opacity hacks on
  palette colors.
- The legacy aliases `background-ui` and `mid-gray` still exist for older
  components. New code must not use them; migrate them away when touching a
  file that does.

## Contrast rules

- Text must meet **>= 4.5:1** against its background (WCAG 2.2 SC 1.4.3).
- UI component boundaries and focus indicators must meet **>= 3:1**
  (WCAG 2.2 SC 1.4.11).
- **Measured rule — accent as text only on untinted background.**
  `text-accent` on the plain background is 4.99:1 in light mode: it passes AA
  with no headroom. `text-accent` on `bg-accent/15` is **4.05:1 — fails AA**.
  Never put accent-colored text (or icons that carry meaning) on
  accent-tinted backgrounds; use `text-text` on tinted surfaces instead.
- The light-mode status colors are contrast-tuned: success `#166534` and
  warning `#854d0e` clear 4.5:1 on both `background` and their `*-bg` tints.
  **Do not lighten them.**

## Scale

Type steps (no arbitrary values like `text-[10px]`):

| Role | Classes |
| --- | --- |
| Page title | `text-xl font-semibold` |
| Group label | `text-xs font-medium uppercase tracking-wide` |
| Row label | `text-sm font-medium` |
| Secondary text | `text-sm text-text-secondary` |

Radius scale:

- `rounded-md` — compact controls: inputs, textareas, dropdown triggers and
  menu items, small icon buttons, key chips, mic level bars.
- `rounded-lg` — buttons and containers: setting rows and groups, cards,
  alerts, tooltips, panels.
- `rounded-full` — toggle switch track and thumb, badges, progress bars, and
  status dots only.
- Nothing else (`rounded-sm`, `rounded-xl`, arbitrary radii) without updating
  this document.

Spacing: 4px base unit. Settings rows are `px-4 py-2`; internals of a control
use `gap-2`; page-level groups are separated with `space-y-6`.

Icons: **lucide-react only** in app chrome, sized 16/20/24 px
(`w-4 h-4` / `w-5 h-5` / `w-6 h-6`), `aria-hidden` when decorative. Custom
SVGs are reserved for the brand mark (`VerbatimMark`, `VerbatimTextLogo`) and
the recording-overlay glyphs.

## Focus

- Every interactive element:
  `focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent`
  (`focus-visible:ring-danger` in destructive contexts).
- No `focus:` ring variants, no ring offsets, no `ring-4`.
- Every icon-only button carries an `aria-label`.

## Feedback rules

- **Blocking problem in the current view** → inline `<Alert>` next to the
  thing that is broken.
- **Transient confirmation** → toast with the default duration.
- **Error with a recovery action** → persistent toast (`duration: Infinity`)
  or an inline alert; it must not auto-dismiss before the user can act.
- Every error message names a cause or a next step. "Something went wrong"
  is not shippable.

## Motion

- Interactive transitions animate **color/opacity only, <= 200ms**.
- No hover scaling, no translate-on-press.
- `prefers-reduced-motion` is honored globally (App.css collapses all
  animation and transition durations).
- The mic level bars are the one expressive element in the app; nothing else
  earns continuous animation.

## Copy

- Sentence case everywhere — labels, buttons, titles.
- Terminology is fixed: **transcript** is the text, **transcription** is the
  process, **dictation** is the act of speaking.
- A description must add information beyond its label; if it only restates
  the label, delete it.
- No marketing adjectives ("powerful", "seamless", "blazing").
- State data flow plainly: say what leaves the machine, where it goes, and
  when.

## i18n

- New keys must be added to **all 20 locales**;
  `bun run check:translations` gates this in CI.
- Changing an English value is safe on its own, but queues that key for
  retranslation in the other 19 locales.
