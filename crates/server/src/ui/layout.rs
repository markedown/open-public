use maud::{html, Markup, DOCTYPE};

use crate::i18n::{self, SITE_NAME};

pub fn document(
    page_title: Option<&str>,
    logged_in: bool,
    is_admin: bool,
    content: Markup,
) -> Markup {
    document_described(page_title, None, logged_in, is_admin, content)
}

/// The same page with a description of its own.
///
/// The description is what a search result and a shared link show underneath
/// the title, so it is worth writing per page rather than leaving a crawler to
/// pick an arbitrary sentence. Pages that have something specific to say pass
/// it; the rest fall back to describing the site.
pub fn document_described(
    page_title: Option<&str>,
    description: Option<&str>,
    logged_in: bool,
    is_admin: bool,
    content: Markup,
) -> Markup {
    let title = match page_title {
        Some(t) => format!("{t} · {SITE_NAME}"),
        None => SITE_NAME.to_string(),
    };
    let description = description.unwrap_or(i18n::t(
        "An open, source-backed record of public political life: who holds office, the parties they belong to, and where they stand. Every fact links to the source it came from.",
    ));
    // The page's own address, so a search engine is told which URL is the one
    // to index and a shared link resolves to the same place it was copied from.
    let canonical = i18n::request_url();
    // Nav links share one monospace, uppercase, underline-on-hover style: the
    // register has no coloured controls.
    let link = "text-ink-muted underline-offset-4 transition-colors hover:text-ink hover:underline";
    html! {
        (DOCTYPE)
        html lang=(i18n::lang_code()) {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (title) }
                meta name="description" content=(description);
                @if let Some(ref url) = canonical {
                    link rel="canonical" href=(url);
                }
                // What a shared link shows: a title, a description and the site
                // it came from. No image is referenced, because there is no
                // hosted one to reference and pointing at a third party would
                // load something from elsewhere.
                meta property="og:type" content="website";
                meta property="og:site_name" content=(SITE_NAME);
                meta property="og:title" content=(title);
                meta property="og:description" content=(description);
                meta property="og:locale" content=(i18n::lang_code());
                @if let Some(ref url) = canonical {
                    meta property="og:url" content=(url);
                }
                meta name="twitter:card" content="summary";
                meta name="twitter:title" content=(title);
                meta name="twitter:description" content=(description);
                link rel="stylesheet" href="/static/app.css";
                link rel="icon" href="/static/brand/favicon.svg" type="image/svg+xml";
                link rel="icon" href="/static/brand/favicon-32.png" sizes="32x32" type="image/png";
                link rel="icon" href="/static/brand/favicon-16.png" sizes="16x16" type="image/png";
                link rel="apple-touch-icon" href="/static/brand/apple-touch-icon.png";
                script src="/static/htmx.min.js" defer {}
            }
            body class="flex min-h-screen flex-col bg-paper font-sans text-ink antialiased" {
                header class="sticky top-0 z-20 border-b border-hairline-strong bg-paper" {
                    nav class="mx-auto flex h-14 max-w-6xl items-center justify-between gap-4 px-4 sm:px-6" {
                        a href="/" class="inline-flex shrink-0 items-center" {
                            img src="/static/brand/wordmark.svg" alt=(SITE_NAME)
                                class="h-[18px] w-auto";
                        }
                        div class="flex items-center gap-4 font-mono text-[11px] uppercase tracking-wide sm:gap-5" {
                            // Language switcher: a native dropdown (no JavaScript).
                            details class="group relative" {
                                summary class="flex cursor-pointer list-none items-center gap-1 text-ink-muted transition-colors hover:text-ink [&::-webkit-details-marker]:hidden" {
                                    (i18n::current().label())
                                    span class="text-[7px] leading-none transition-transform group-open:rotate-180" { "▼" }
                                }
                                div class="absolute right-0 z-30 mt-2 min-w-[9rem] border border-hairline bg-paper-raised py-1" {
                                    @for lang in i18n::Lang::ALL {
                                        a href={"/lang/" (lang.code())}
                                          class=(if lang.is_active() {
                                              "flex items-center justify-between gap-3 px-3 py-1.5 text-[11px] text-ink"
                                          } else {
                                              "flex items-center justify-between gap-3 px-3 py-1.5 text-[11px] text-ink-muted transition-colors hover:bg-paper-sunken hover:text-ink"
                                          }) {
                                            span class="normal-case" { (lang.name()) }
                                            span { (lang.label()) }
                                        }
                                    }
                                }
                            }
                            a href="/search" class=(link) { (i18n::t("Search")) }
                            @if is_admin {
                                a href="/admin" class=(link) { (i18n::t("Admin panel")) }
                            }
                            @if logged_in {
                                a href="/feed" class=(link) { (i18n::t("Feed")) }
                                a href="/submissions" class=(link) { (i18n::t("My submissions")) }
                                form method="post" action="/logout" {
                                    button type="submit" class=(link) { (i18n::t("Log out")) }
                                }
                            } @else {
                                a href="/login" class=(link) { (i18n::t("Log in")) }
                                a href="/register"
                                   class="border border-ink bg-ink px-3.5 py-1.5 font-semibold text-paper transition-colors hover:bg-paper hover:text-ink" {
                                    (i18n::t("Register"))
                                }
                            }
                        }
                    }
                }
                main class="mx-auto w-full max-w-6xl grow px-4 py-8 sm:px-6 sm:py-12" {
                    (content)
                }
                footer class="mt-16 border-t border-hairline-strong" {
                    div class="mx-auto flex max-w-6xl flex-wrap items-center justify-between gap-3 px-4 py-5 font-mono text-[11px] uppercase tracking-wide text-ink-faint sm:px-6" {
                        span { (i18n::t("Open political data.")) }
                        nav class="flex gap-4" {
                            a href="/search" class="transition-colors hover:text-ink" { (i18n::t("Search")) }
                            a href="/privacy" class="transition-colors hover:text-ink" { (i18n::t("Privacy")) }
                        }
                    }
                }
            }
        }
    }
}
