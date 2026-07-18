# Design system

The reference for anyone building UI in the `server` crate. The interface is a restrained civic
dashboard: a calm neutral surface carries one brand accent, and organization color stays data.

Reusable UI is our own `maud` component functions in the `ui` module (layout, badge, button, source
link, timeline entry, poll widget). One definition per component; pages compose them and never restyle
ad hoc.

## Principles

- **Neutral surface, one brand accent, color is data.** Chrome is a cool slate neutral scale (near-black
  slate text, white cards on a slate-50 ground). A single indigo brand accent (`oklch(51% 0.20 277)`,
  about `#5b57d8`) carries links, primary actions and focus states. It is deliberately distinct from
  every organization color, so chrome never reads as a political allegiance, and it is never mapped to a
  specific party. Organization colors (party badges, seat bars, charts) appear only inside data
  elements, never as general chrome.
- **Cards and depth.** Content sits on rounded card surfaces (`op-card` / `op-card-link` in the `ui`
  module) with a hairline border and a soft shadow that lifts slightly on hover for links. Sections use
  the shared `section_header` / `page_header` helpers so every heading and see-all link reads the same.
- **Tone:** a calm public-infrastructure dashboard, like a statistics office with good taste. Restrained
  depth only (soft card shadows, a frosted sticky header); no gradients, no marketing styling, no emoji
  in UI copy (the home construction notice is the one intentional exception).
- **Typography:** self-hosted fonts only (no web-font CDN, for privacy). Public Sans is the interface
  and display face, used for the wordmark and all headings; IBM Plex Mono marks verifiable figures
  (counts, dates, citation chits). The interface font must render the active locale's glyphs correctly.
  Comfortable reading: body line length around 70ch, at most three text sizes per page, clear hierarchy.
- **Layout:** mobile-first, content-first, on a `max-w-6xl` frame. The country page opens compact: a
  hero card with identity, key facts and counted pill navigation, then polls, government, legislature,
  coalitions and elections, each previewing a few items with a see-all link to the full index. Person
  and party pages are the core records: an identity card at the top (photo, name, current party badge,
  current role), then a vertical timeline of roles and party memberships, then polls and news. Lists are
  dense but scannable.
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
