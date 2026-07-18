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
/// vote, each result row is itself the vote control (tap to vote). `voted` holds
/// the option ids this viewer already chose, so their own answer is marked and
/// compared with the crowd.
pub fn poll_widget(poll: &Poll, viewer: Viewer, country: &str, voted: &[i64]) -> Markup {
    let total: i64 = poll.options.iter().map(|o| o.votes).sum();
    let action = format!("/{}/poll/{}/vote", country, poll.slug);
    let target = format!("#poll-{}", poll.slug);
    let can_vote = matches!(viewer, Viewer::CanVote);

    html! {
        article id={"poll-" (poll.slug)} class="op-card p-6 sm:p-8" {
            h3 class="text-2xl font-bold leading-snug tracking-tight text-ink" { (poll.question) }

            @if let Some(ref url) = poll.media_url {
                figure class="mt-4" {
                    img src=(url) alt="" loading="lazy"
                        class="max-h-56 w-full rounded-lg border border-hairline object-contain";
                    @if let Some(ref lic) = poll.media_license {
                        figcaption class="mt-1 font-mono text-[10px] text-ink-muted" { (lic) }
                    }
                }
            }

            @match poll.kind.as_str() {
                "multi" if can_vote => (multi_options(poll, total, &action, &target)),
                "yesno" => (grid_options(poll, total, can_vote, &action, &target, voted, "mt-6 grid grid-cols-2 gap-3")),
                "scale" => (grid_options(poll, total, can_vote, &action, &target, voted, "mt-6 flex flex-wrap gap-2")),
                _ => (stacked_options(poll, total, can_vote, &action, &target, voted)),
            }

            // The personal layer: after voting, show where the viewer stands.
            (your_take(poll, total, voted))

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
                    a href="/login" class="font-medium text-accent hover:underline" { (i18n::t("Log in to vote.")) }
                },
            }
        }
    }
}

/// Read-only poll previews for a country, person or party page: each poll is a
/// card with its question and current results, linking to the full poll. Shows
/// at most `limit` cards, laid out two-up, with an optional "see all" link and
/// admin add affordance in the header. Renders nothing when there is nothing to
/// show.
pub fn poll_previews(
    polls: &[Poll],
    country: &str,
    limit: Option<usize>,
    see_all: Option<&str>,
    add_href: Option<&str>,
) -> Markup {
    if polls.is_empty() && add_href.is_none() {
        return html! {};
    }
    let shown: &[Poll] = match limit {
        Some(n) if polls.len() > n => &polls[..n],
        _ => polls,
    };
    let action = html! {
        div class="flex items-center gap-4" {
            @if let Some(href) = add_href {
                a href=(href) class="text-[12px] font-semibold text-accent transition-colors hover:underline" {
                    "+ " (i18n::t("Add poll"))
                }
            }
            @if let Some(href) = see_all { (crate::ui::see_all_link(href)) }
        }
    };
    html! {
        section class="mb-8" {
            (crate::ui::section_header(i18n::t("Polls"), Some(action)))
            @if polls.is_empty() {
                p class="text-sm text-ink-muted" { (i18n::t("No polls yet.")) }
            } @else {
                div class="grid gap-4 sm:grid-cols-2" {
                    @for poll in shown {
                        (poll_preview_card(poll, country))
                    }
                }
            }
        }
    }
}

/// One poll rendered as a preview card: the question, its option tallies, and
/// the total vote count, the whole card linking to the poll's own page.
fn poll_preview_card(poll: &Poll, country: &str) -> Markup {
    let total: i64 = poll.options.iter().map(|o| o.votes).sum();
    html! {
        a href={"/" (country) "/poll/" (poll.slug)}
          class="op-card op-card-link flex flex-col p-5" {
            span class="text-[15px] font-semibold leading-snug text-ink" { (poll.question) }
            div class="mt-4 space-y-1.5" {
                @for opt in &poll.options {
                    @let pct = if total > 0 { opt.votes * 100 / total } else { 0 };
                    div class="relative overflow-hidden rounded-md bg-paper-sunken" {
                        (result_row(&opt.label, pct, opt.votes, opt.media_url.as_deref(), false))
                    }
                }
            }
            p class="mt-3 font-mono text-[11px] text-ink-muted" {
                (format!("{} {}", total, i18n::t("votes")))
            }
        }
    }
}

/// The fill-behind-label content of one result row: an accent-tint bar sized to
/// the percentage, with the label and the figures on one line. `mine` marks the
/// row the viewer voted for.
fn result_row(label: &str, pct: i64, votes: i64, media: Option<&str>, mine: bool) -> Markup {
    html! {
        div class="absolute inset-y-0 left-0 bg-accent-tint" style={"width:" (pct) "%"} {}
        div class="relative flex items-center justify-between gap-3 px-3 py-2.5 text-sm" {
            div class="flex min-w-0 items-center gap-3" {
                @if mine { span class="shrink-0 font-bold text-accent" { "✓" } }
                @if let Some(url) = media {
                    img src=(url) alt="" loading="lazy"
                        class="h-9 w-9 shrink-0 rounded border border-hairline object-cover";
                }
                span class=(if mine { "truncate font-semibold text-accent" } else { "truncate text-ink" }) { (label) }
            }
            span class="shrink-0 font-mono text-xs font-semibold text-ink" { (pct) "% · " (votes) }
        }
    }
}

/// The personal layer shown once a viewer has voted: their own pick, its share,
/// and whether it is the leading answer. Renders nothing before voting.
fn your_take(poll: &Poll, total: i64, voted: &[i64]) -> Markup {
    if voted.is_empty() || total == 0 {
        return html! {};
    }
    let leader = poll.options.iter().max_by_key(|o| o.votes).map(|o| o.id);
    html! {
        div class="mt-4 rounded-lg border border-accent/30 bg-accent-tint px-4 py-3 text-sm text-ink" {
            @for opt in poll.options.iter().filter(|o| voted.contains(&o.id)) {
                @let pct = if total > 0 { opt.votes * 100 / total } else { 0 };
                p class="flex flex-wrap items-baseline gap-x-1.5" {
                    span class="font-semibold text-accent" { (i18n::t("Your pick")) ":" }
                    span class="font-medium" { (opt.label) }
                    span class="font-mono text-xs text-ink-muted" { "· " (pct) "%" }
                    @if Some(opt.id) == leader {
                        span class="text-xs text-ink-muted" { "· " (i18n::t("the leading answer")) }
                    }
                }
            }
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
/// scale's fallback). `voted` marks the viewer's own choice.
fn stacked_options(
    poll: &Poll,
    total: i64,
    can_vote: bool,
    action: &str,
    target: &str,
    voted: &[i64],
) -> Markup {
    html! {
        div class="mt-6 space-y-2.5" {
            @for opt in &poll.options {
                @let pct = if total > 0 { opt.votes * 100 / total } else { 0 };
                @let mine = voted.contains(&opt.id);
                (option_control(
                    can_vote, action, target, opt.id,
                    &format!(
                        "relative block w-full overflow-hidden rounded-lg border text-left transition-colors hover:border-accent {}",
                        if mine { "border-accent ring-1 ring-accent" } else { "border-hairline" },
                    ),
                    result_row(&opt.label, pct, opt.votes, opt.media_url.as_deref(), mine),
                ))
            }
        }
    }
}

/// Yes/no and scale layout: option cells laid out by `container` (a 2-column
/// grid, or a wrapping row), each showing its label above the figures with the
/// tally bar behind. `voted` marks the viewer's own choice.
fn grid_options(
    poll: &Poll,
    total: i64,
    can_vote: bool,
    action: &str,
    target: &str,
    voted: &[i64],
    container: &str,
) -> Markup {
    html! {
        div class=(container) {
            @for opt in &poll.options {
                @let pct = if total > 0 { opt.votes * 100 / total } else { 0 };
                @let mine = voted.contains(&opt.id);
                (option_control(
                    can_vote, action, target, opt.id,
                    &format!(
                        "relative grow basis-24 overflow-hidden rounded-lg border px-3 py-4 text-center transition-colors hover:border-accent {}",
                        if mine { "border-accent ring-1 ring-accent" } else { "border-hairline" },
                    ),
                    html! {
                        div class="absolute inset-y-0 left-0 bg-accent-tint" style={"width:" (pct) "%"} {}
                        div class="relative" {
                            @if mine {
                                div class="mb-1 text-[10px] font-bold uppercase tracking-wide text-accent" { "✓ " (i18n::t("Your pick")) }
                            }
                            @if let Some(url) = opt.media_url.as_deref() {
                                img src=(url) alt="" loading="lazy"
                                    class="mx-auto mb-2 h-20 w-20 rounded border border-hairline object-cover";
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
                    label class="relative flex cursor-pointer items-center gap-3 overflow-hidden rounded-lg border border-hairline px-4 py-3 transition-colors hover:border-accent" {
                        div class="absolute inset-y-0 left-0 bg-accent-tint" style={"width:" (pct) "%"} {}
                        input type="checkbox" name="option_id" value=(opt.id)
                            class="relative z-10 h-4 w-4 shrink-0 rounded border border-hairline accent-accent";
                        @if let Some(url) = opt.media_url.as_deref() {
                            img src=(url) alt="" loading="lazy"
                                class="relative z-10 h-9 w-9 shrink-0 rounded border border-hairline object-cover";
                        }
                        span class="relative z-10 grow text-sm text-ink" { (opt.label) }
                        span class="relative z-10 shrink-0 font-mono text-sm font-semibold text-ink" {
                            (pct) "% · " (opt.votes)
                        }
                    }
                }
            }
            button type="submit"
                class="mt-3 rounded-lg bg-accent px-4 py-2 text-[12px] font-semibold text-white shadow-sm transition-colors hover:bg-accent-strong" {
                (i18n::t("Vote"))
            }
        }
    }
}
