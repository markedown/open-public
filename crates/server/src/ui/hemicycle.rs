//! The chamber: a parliament's seat composition drawn as a party-coloured
//! half-donut. This is the platform's signature, the single place colour
//! appears (colour is data), and the most characteristic artifact of the
//! subject: who holds how many seats.

use maud::{html, Markup, PreEscaped};

use crate::i18n;

/// Neutral grey for independents, deliberately not any party colour.
const INDEPENDENT_COLOR: &str = "oklch(62% 0.01 70)";

/// A seat-composition hemicycle. `seats` are the parties (each a coloured
/// wedge, in the order given), `independents` a neutral wedge, and `vacant` an
/// optional faint wedge for empty seats. `size` is the SVG width in user units.
/// Renders nothing without seats.
pub fn hemicycle(
    seats: &[db::country::SeatCount],
    independents: i64,
    vacant: Option<i64>,
    size: f64,
) -> Markup {
    let total: i64 =
        seats.iter().map(|s| s.seats).sum::<i64>() + independents + vacant.unwrap_or(0);
    if total <= 0 {
        return html! {};
    }

    let w = size;
    let cx = w / 2.0;
    let outer = w / 2.0 - 1.0;
    let inner = outer * 0.52;
    let cy = outer + 1.0; // baseline; the arc opens upward
    let h = cy + 1.0;

    // Wedges in reading order: parties (seat-desc as given), independents, then
    // vacant. Colours are carried in a `fill:` style so party hex and the theme
    // vars for independents and vacant both resolve.
    let mut segs: Vec<(i64, String)> = seats
        .iter()
        .filter(|s| s.seats > 0)
        .map(|s| {
            (
                s.seats,
                s.color.clone().unwrap_or_else(|| "#171717".to_string()),
            )
        })
        .collect();
    if independents > 0 {
        segs.push((independents, INDEPENDENT_COLOR.to_string()));
    }
    if let Some(v) = vacant.filter(|v| *v > 0) {
        segs.push((v, "var(--color-paper-sunken)".to_string()));
    }

    // A seat fraction maps to an angle from 180 degrees (left) to 0 (right).
    let pt = |r: f64, frac: f64| {
        let a = (180.0 - frac * 180.0) * std::f64::consts::PI / 180.0;
        (cx + r * a.cos(), cy - r * a.sin())
    };

    let mut paths = String::new();
    let mut cum = 0i64;
    for (count, color) in &segs {
        let a0 = cum as f64 / total as f64;
        let a1 = (cum + count) as f64 / total as f64;
        let (ox0, oy0) = pt(outer, a0);
        let (ox1, oy1) = pt(outer, a1);
        let (ix1, iy1) = pt(inner, a1);
        let (ix0, iy0) = pt(inner, a0);
        paths.push_str(&format!(
            "<path d=\"M{ox0:.2} {oy0:.2} A{outer:.2} {outer:.2} 0 0 1 {ox1:.2} {oy1:.2} \
             L{ix1:.2} {iy1:.2} A{inner:.2} {inner:.2} 0 0 0 {ix0:.2} {iy0:.2} Z\" \
             style=\"fill:{color};stroke:var(--color-paper-raised);stroke-width:1.25\"/>"
        ));
        cum += count;
    }

    html! {
        svg viewBox={"0 0 " (w) " " (h)} class="block w-full overflow-visible"
            role="img" aria-label=(i18n::t("Seats")) preserveAspectRatio="xMidYMax meet" {
            (PreEscaped(paths))
        }
    }
}
