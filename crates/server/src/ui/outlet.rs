use maud::{html, Markup};

use crate::i18n;

/// A localized label for a leaning value on the five-point spectrum.
pub fn leaning_label(leaning: &str) -> &'static str {
    match leaning {
        "left" => i18n::t("Left"),
        "lean_left" => i18n::t("Lean left"),
        "center" => i18n::t("Center"),
        "lean_right" => i18n::t("Lean right"),
        "right" => i18n::t("Right"),
        _ => i18n::t("Unrated"),
    }
}

/// The position of a leaning on the spectrum, if known.
fn leaning_index(leaning: &str) -> Option<usize> {
    db::outlets::LEANINGS.iter().position(|&l| l == leaning)
}

/// A five-cell spectrum with the outlet's position filled. Position encodes the
/// leaning; the track stays monochrome so no cell reads as a party colour. A
/// compact variant drops the caption for list rows.
pub fn leaning_bar(leaning: &str, compact: bool) -> Markup {
    let active = leaning_index(leaning);
    html! {
        div class="inline-flex flex-col gap-1" {
            div class="flex gap-0.5" title=(leaning_label(leaning)) {
                @for i in 0..db::outlets::LEANINGS.len() {
                    div class={
                        "h-2 w-5 rounded-sm border border-hairline "
                        (if Some(i) == active { "bg-ink" } else { "bg-transparent" })
                    } {}
                }
            }
            @if !compact {
                span class="text-[10px] font-bold uppercase tracking-widest text-ink-muted" {
                    (leaning_label(leaning))
                }
            }
        }
    }
}

/// One outlet row for the index: logo or monogram, name, leaning, article count.
pub fn card(o: &db::outlets::OutletCard, country: &str) -> Markup {
    html! {
        a href={"/" (country) "/outlet/" (o.slug)}
          class="op-card op-card-link flex items-center gap-4 px-4 py-3" {
            @if let Some(ref logo) = o.logo_url {
                img src=(logo) alt="" loading="lazy"
                    class="h-8 w-8 shrink-0 rounded border border-hairline object-contain";
            } @else {
                span class="flex h-8 w-8 shrink-0 items-center justify-center rounded border border-hairline bg-paper-sunken font-mono text-[11px] font-semibold text-ink-muted" {
                    (crate::ui::initials(&o.name))
                }
            }
            span class="grow text-sm font-medium text-ink" { (o.name) }
            @if let Some(ref l) = o.leaning {
                (leaning_bar(l, true))
            }
            span class="shrink-0 font-mono text-xs text-ink-muted" { (o.article_count) }
        }
    }
}
