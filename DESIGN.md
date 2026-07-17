# Design system

The reference for anyone building UI in the `server` crate. The interface is neutral; color is data.

Reusable UI is our own `maud` component functions in the `ui` module (layout, badge, button, source
link, timeline entry, poll widget). One definition per component; pages compose them and never restyle
ad hoc.

## Principles

- **Neutral chrome, color is data.** The chrome is near-monochrome (a neutral gray scale, near-black
  text on near-white). Organization colors (such as party badges) appear only inside data elements,
  never as interface accent. Never use an accent that maps to a specific political party. If an accent
  is needed for links and focus states, use a desaturated ink-blue (`#33527a`), sparingly.
- **Tone:** a sober public-infrastructure look, like a statistics office with good taste. No gradients,
  no glassmorphism, no marketing styling, no emoji in UI text.
- **Typography:** self-hosted fonts only (no web-font CDN, for privacy): a neutral sans for the
  interface (Public Sans) and a distinctive serif for the wordmark (Spectral, converted to paths). The
  interface font must render the active locale's glyphs correctly. Comfortable reading: body line
  length around 70ch, at most three text sizes per page, clear hierarchy.
- **Layout:** mobile-first, content-first. Person and party pages are the core: an identity card at the
  top (photo, name, current party badge, current role), then a vertical timeline of roles and party
  memberships, then polls, then news. Lists are dense but scannable.
- **Sources are first-class UI.** Every sourced fact shows a small, unobtrusive source link next to it,
  never hidden behind a hover-only affordance. This is the product's credibility.
- **The informal poll label** is always visible on poll widgets and results, styled clearly but not
  alarmist.
- **Accessibility:** semantic HTML (`nav`, `main`, `article`, `time`), WCAG AA contrast, visible focus
  states, alt text on every photo (the person's name), forms with proper labels, correct `lang`
  attribute on `<html>`.
- **Performance:** no JavaScript beyond `htmx.min.js`; images lazy-loaded and size-constrained; pages
  usable on slow mobile connections.
- **No dark mode in v1.**
