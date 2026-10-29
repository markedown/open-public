# Design system

The reference for anyone building UI in the `server` crate. The interface is friendly civic
information: it explains before it shows numbers, reads like a good public-information service, and
stays politically neutral. Color is data.

Reusable UI is our own `maud` component functions in the `ui` module (layout, badge, button, source
link, timeline entry, poll widget). One definition per component; pages compose them and never restyle
ad hoc.

## Principles

- **Explain first.** A page opens with a plain sentence that says what matters ("The list led by X won
  129 of 250 seats"), then the numbers. Section headings are sentence case and often the question the
  section answers: "Who governs?", "Who sits in parliament?", "When is the next election?". Terms a
  visitor may not know (a threshold, a seat method) get a short explanation box in everyday language.
- **Every fact shows its source.** A small numbered marker sits next to each sourced fact and links to
  a source list at the end of the page ("Where this comes from"), with the publisher and the date it was
  retrieved. Never hidden behind hover. This is the product's credibility.
- **Honest status.** Something not yet settled says so on the page: a count still running, a date not
  yet set, a figure we chose not to take from a preliminary result. These notices use the pending tone,
  never alarm colors.
- **Color is data.** The chrome is slate on white with a pale blue-grey for bands and explanations.
  Party colors appear only inside data elements (seat dots, party dots, bars). A party with no
  documented color is shown in neutral grey, never in a color we picked. Actions are slate, not a brand
  hue, because every hue is some party's color somewhere.
- **Links look like links.** Content links are blue (`--color-link`) and underlined, thicker on hover
  (`op-link`). That convention is the one hue the chrome uses.
- **Participation is honest.** Voting and approval blocks are inviting, and always say that the vote is
  anonymous and that results show how participants here voted, not a representative survey.

## Tokens

Defined once in `crates/server/assets/tailwind.css` (`@theme`):

| Token | Value | Use |
|---|---|---|
| `paper` | `#ffffff` | page ground, cards |
| `paper-sunken` | `#f2f5f9` | hero bands, side panels, tracks, footer |
| `ink` | `#1b1f24` | text, primary actions |
| `ink-muted` | `#545b64` | secondary text (7:1 on white) |
| `ink-faint` | `#69717a` | meta (4.9:1 on white) |
| `hairline` | `#dfe3e8` | borders |
| `hairline-light` | `#eef1f4` | row separators |
| `hairline-strong` | `#b9c0c8` | inputs, rings |
| `link` | `#1f5fbf` | content links |
| `accent-tint` | `#e9f0f8` | explanation and notice boxes |
| `pending` | `#fff3bf` | a status still open ("result follows") |
| `focus` | `#ffd84d` | keyboard focus fill, with a dark outline |

Surfaces have a 12 px radius (`--radius-card`, `op-card`), buttons and inputs 8 to 10 px, status pills
are fully rounded. No shadows on content, no gradients, no emoji in UI copy.

## Typography

One self-hosted typeface: **Source Sans 3** (SIL OFL), variable weight, one `woff2` per script subset
(Latin, Latin Extended, Cyrillic) in `crates/server/static/fonts/`, with its license next to it. The
browser fetches only the subsets a page uses. Figures use tabular numerals, set on the body, so counts
align without a monospace font.

- Page title 40 to 50 px, extra bold, tight.
- Section heading 26 to 30 px, extra bold.
- Lead sentence 20 to 21 px.
- Body 17 to 18 px, secondary 15 to 16 px. Nothing below 13 px.
- No uppercase letter-spaced labels: a label is a short sentence-case phrase in the secondary color.

## Layout and components

- `max-w-6xl` frame, mobile-first. Detail pages open with a pale hero band: breadcrumb, title, the lead
  sentence, then up to three large "at a glance" figures on white cards.
- Sections are separated by space and a heading, not by boxes. Cards are for things you can click
  (people, elections) or for a contained explanation.
- A chamber is shown as dots, one per seat, grouped by party, next to a list of parties with their dot
  and seat count. Independents are hollow dots.
- Events in time (roles, elections) are a vertical timeline: a line, a dot per entry, date above title.
- Result bars are rounded, dark for contestants that won seats and grey for those that did not, with
  the threshold drawn as a dashed line.
- People appear with a round portrait when a freely licensed photo exists, otherwise a neutral
  silhouette.
- The header carries the search field on every page; the primary action (register) is the only filled
  control in it.

## Accessibility

Semantic HTML (`nav`, `main`, `article`, `time`), WCAG AA contrast, visible focus (yellow fill with a
dark outline, the same everywhere), touch targets of at least 44 px, alt text on every photo (the
person's name), labelled form fields, decorative SVG marked `aria-hidden` and `focusable="false"`, and
the correct `lang` on `<html>` (including the script, such as `sr-Latn`).

## Performance

JavaScript kept small and vendored (HTMX, the anonymous-voting island, the proof-of-work captcha),
never a client framework or a CDN; images lazy-loaded and size-constrained; one font family split by
script. Pages stay usable on slow mobile connections.

No dark mode yet.
