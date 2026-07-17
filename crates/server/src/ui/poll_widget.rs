use domain::models::Poll;
use maud::{html, Markup};

use crate::i18n;

/// Who is looking at the poll, which decides whether the options are votable.
pub enum Viewer {
    Anonymous,
    CanVote,
    Voted,
}

/// The poll on its own page. Results are always visible; when the viewer can
/// vote, each result row is itself the vote control (tap to vote).
pub fn poll_widget(poll: &Poll, viewer: Viewer, country: &str) -> Markup {
    let total: i64 = poll.options.iter().map(|o| o.votes).sum();
    let action = format!("/{}/poll/{}/vote", country, poll.slug);
    let target = format!("#poll-{}", poll.slug);
    let can_vote = matches!(viewer, Viewer::CanVote);

    html! {
        article id={"poll-" (poll.slug)}
                class={"border-[1.5px] border-ink bg-paper-raised p-7 " (crate::ui::CORNER_TICK)} {
            h3 class="font-serif text-2xl font-medium leading-snug text-ink" { (poll.question) }

            @if let Some(ref url) = poll.media_url {
                figure class="mt-4" {
                    img src=(url) alt="" loading="lazy"
                        class="max-h-56 w-full border border-hairline object-contain";
                    @if let Some(ref lic) = poll.media_license {
                        figcaption class="mt-1 font-mono text-[10px] text-ink-muted" { (lic) }
                    }
                }
            }

            @match poll.kind.as_str() {
                "multi" if can_vote => (multi_options(poll, total, &action, &target)),
                "yesno" => (grid_options(poll, total, can_vote, &action, &target, "mt-6 grid grid-cols-2 gap-3")),
                "scale" => (grid_options(poll, total, can_vote, &action, &target, "mt-6 flex flex-wrap gap-2")),
                _ => (stacked_options(poll, total, can_vote, &action, &target)),
            }

            p class="mt-4 font-mono text-xs text-ink-muted" {
                (format!("{} {}", total, i18n::t("votes")))
            }
            @match viewer {
                Viewer::Voted => p class="mt-1 font-mono text-xs text-ink-muted" { (i18n::t("You have voted.")) },
                Viewer::CanVote => p class="mt-1 text-xs text-ink-muted" {
                    @if poll.kind == "multi" { (i18n::t("Select one or more options, then vote.")) }
                    @else { (i18n::t("Tap an option to vote.")) }
                },
                Viewer::Anonymous => p class="mt-1 text-sm text-ink-muted" {
                    a href="/login" class="text-accent hover:underline" { (i18n::t("Log in to vote.")) }
                },
            }
        }
    }
}

/// Read-only poll previews for a person or party page: question plus current
/// results, each linking to the full poll. Renders nothing when there are none
/// and no admin add affordance.
pub fn poll_previews(polls: &[Poll], country: &str, add_href: Option<&str>) -> Markup {
    if polls.is_empty() && add_href.is_none() {
        return html! {};
    }
    html! {
        section class="mb-12" {
            div class="mb-5 flex items-center justify-between gap-3 border-b-2 border-accent pb-2" {
                h2 class="text-xs font-bold uppercase tracking-widest text-ink" { (i18n::t("Polls")) }
                @if let Some(href) = add_href {
                    a href=(href) class="text-[11px] font-bold uppercase tracking-wide text-accent transition-colors hover:underline" {
                        "+ " (i18n::t("Add poll"))
                    }
                }
            }
            @if polls.is_empty() {
                p class="text-sm text-ink-muted" { (i18n::t("No polls yet.")) }
            }
            div class="space-y-5" {
                @for poll in polls {
                    @let total: i64 = poll.options.iter().map(|o| o.votes).sum();
                    a href={"/" (country) "/poll/" (poll.slug)}
                      class="block border border-hairline p-4 transition-colors hover:border-ink" {
                        span class="text-sm font-medium text-ink" { (poll.question) }
                        div class="mt-3 space-y-1.5" {
                            @for opt in &poll.options {
                                @let pct = if total > 0 { opt.votes * 100 / total } else { 0 };
                                div class="relative overflow-hidden border border-hairline-light" {
                                    (result_row(&opt.label, pct, opt.votes, opt.media_url.as_deref()))
                                }
                            }
                        }
                        p class="mt-2 font-mono text-[11px] text-ink-muted" {
                            (format!("{} {}", total, i18n::t("votes")))
                        }
                    }
                }
            }
        }
    }
}

/// The fill-behind-label content of one result row: an accent-tint bar sized to
/// the percentage, with the label and the figures on one line.
fn result_row(label: &str, pct: i64, votes: i64, media: Option<&str>) -> Markup {
    html! {
        div class="absolute inset-y-0 left-0 bg-accent-tint" style={"width:" (pct) "%"} {}
        div class="relative flex items-center justify-between gap-3 px-4 py-3 text-sm" {
            div class="flex min-w-0 items-center gap-3" {
                @if let Some(url) = media {
                    img src=(url) alt="" loading="lazy"
                        class="h-9 w-9 shrink-0 border border-hairline object-cover";
                }
                span class="truncate text-ink" { (label) }
            }
            span class="shrink-0 font-mono font-semibold text-ink" { (pct) "% · " (votes) }
        }
    }
}

/// Wrap one option as a vote control (a per-option form, HTMX-enhanced with a
/// plain-POST fallback) when the viewer can vote, or a plain container when
/// results are read-only. `class` styles the control either way.
fn option_control(
    can_vote: bool,
    action: &str,
    target: &str,
    option_id: i64,
    class: &str,
    inner: Markup,
) -> Markup {
    html! {
        @if can_vote {
            form method="post" action=(action) hx-post=(action) hx-target=(target)
                 hx-swap="outerHTML" class="contents" {
                input type="hidden" name="option_id" value=(option_id);
                button type="submit" class={"cursor-pointer " (class)} { (inner) }
            }
        } @else {
            div class=(class) { (inner) }
        }
    }
}

/// Single-choice layout: full-width stacked bars (also used for yes/no and
/// scale's fallback).
fn stacked_options(poll: &Poll, total: i64, can_vote: bool, action: &str, target: &str) -> Markup {
    html! {
        div class="mt-6 space-y-2.5" {
            @for opt in &poll.options {
                @let pct = if total > 0 { opt.votes * 100 / total } else { 0 };
                (option_control(
                    can_vote, action, target, opt.id,
                    "relative block w-full overflow-hidden border border-hairline text-left transition-colors hover:border-ink",
                    result_row(&opt.label, pct, opt.votes, opt.media_url.as_deref()),
                ))
            }
        }
    }
}

/// Yes/no and scale layout: option cells laid out by `container` (a 2-column
/// grid, or a wrapping row), each showing its label above the figures with the
/// tally bar behind.
fn grid_options(
    poll: &Poll,
    total: i64,
    can_vote: bool,
    action: &str,
    target: &str,
    container: &str,
) -> Markup {
    html! {
        div class=(container) {
            @for opt in &poll.options {
                @let pct = if total > 0 { opt.votes * 100 / total } else { 0 };
                (option_control(
                    can_vote, action, target, opt.id,
                    "relative grow basis-24 overflow-hidden border-[1.5px] border-ink px-3 py-4 text-center transition-colors hover:border-accent",
                    html! {
                        div class="absolute inset-y-0 left-0 bg-accent-tint" style={"width:" (pct) "%"} {}
                        div class="relative" {
                            @if let Some(url) = opt.media_url.as_deref() {
                                img src=(url) alt="" loading="lazy"
                                    class="mx-auto mb-2 h-20 w-20 border border-hairline object-cover";
                            }
                            div class="text-sm font-semibold text-ink" { (opt.label) }
                            div class="mt-1 font-mono text-xs text-ink-muted" { (pct) "% · " (opt.votes) }
                        }
                    },
                ))
            }
        }
    }
}

/// Multi-select layout: one form with a checkbox per option and a single
/// submit, so a voter casts several options at once (HTMX-enhanced, plain-POST
/// fallback). Shown only while the viewer can still vote.
fn multi_options(poll: &Poll, total: i64, action: &str, target: &str) -> Markup {
    html! {
        form method="post" action=(action) hx-post=(action) hx-target=(target)
             hx-swap="outerHTML" class="mt-6" {
            div class="space-y-2.5" {
                @for opt in &poll.options {
                    @let pct = if total > 0 { opt.votes * 100 / total } else { 0 };
                    label class="relative flex cursor-pointer items-center gap-3 overflow-hidden border border-hairline px-4 py-3 transition-colors hover:border-ink" {
                        div class="absolute inset-y-0 left-0 bg-accent-tint" style={"width:" (pct) "%"} {}
                        input type="checkbox" name="option_id" value=(opt.id)
                            class="relative z-10 h-4 w-4 shrink-0 border border-ink accent-ink";
                        @if let Some(url) = opt.media_url.as_deref() {
                            img src=(url) alt="" loading="lazy"
                                class="relative z-10 h-9 w-9 shrink-0 border border-hairline object-cover";
                        }
                        span class="relative z-10 grow text-sm text-ink" { (opt.label) }
                        span class="relative z-10 shrink-0 font-mono text-sm font-semibold text-ink" {
                            (pct) "% · " (opt.votes)
                        }
                    }
                }
            }
            button type="submit"
                class="mt-3 border border-ink bg-ink px-4 py-2 text-[11px] font-bold uppercase tracking-wide text-paper transition-colors hover:border-accent hover:bg-accent" {
                (i18n::t("Vote"))
            }
        }
    }
}
