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
    // The card image, absolute because a platform rendering the preview has no
    // page to resolve a relative path against. Omitted, like the canonical URL,
    // when the deployment has not been told its own origin.
    let og_image = i18n::site_origin().map(|o| format!("{o}/static/brand/og.png"));
    // Header links are plain text that darkens and underlines on hover; the one
    // filled control is the primary action (register).
    let link = "rounded-md px-1 py-1 text-ink-muted underline-offset-4 transition-colors hover:text-ink hover:underline";
    html! {
        (DOCTYPE)
        html lang=(i18n::html_tag()) {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (title) }
                meta name="description" content=(description);
                @if let Some(ref url) = canonical {
                    link rel="canonical" href=(url);
                }
                // What a shared link shows: a title, a description, a card
                // image and the site it came from. The image is our own file on
                // our own origin; a preview is fetched by the platform that
                // renders the card, never by the page, so nothing here makes a
                // visitor load anything from elsewhere.
                meta property="og:type" content="website";
                meta property="og:site_name" content=(SITE_NAME);
                meta property="og:title" content=(title);
                meta property="og:description" content=(description);
                meta property="og:locale" content=(i18n::lang_code());
                @if let Some(ref url) = canonical {
                    meta property="og:url" content=(url);
                }
                @if let Some(ref img) = og_image {
                    meta property="og:image" content=(img);
                    meta property="og:image:width" content="1200";
                    meta property="og:image:height" content="630";
                    meta property="og:image:alt" content=(SITE_NAME);
                    meta name="twitter:image" content=(img);
                }
                // A large card once there is an image worth showing; the plain
                // summary card is what a platform falls back to without one.
                meta name="twitter:card" content=(if og_image.is_some() {
                    "summary_large_image"
                } else {
                    "summary"
                });
                meta name="twitter:title" content=(title);
                meta name="twitter:description" content=(description);
                link rel="stylesheet" href="/static/app.css";
                link rel="icon" href="/static/brand/favicon.svg" type="image/svg+xml";
                link rel="icon" href="/static/brand/favicon-32.png" sizes="32x32" type="image/png";
                link rel="icon" href="/static/brand/favicon-16.png" sizes="16x16" type="image/png";
                link rel="apple-touch-icon" href="/static/brand/apple-touch-icon.png";
                script src="/static/htmx.min.js" defer {}
            }
            body class="flex min-h-screen flex-col overflow-x-clip bg-paper font-sans text-[17px] leading-relaxed text-ink antialiased" {
                // The first thing a keyboard user reaches: a way past the nav
                // straight to the page content. Off-screen until focused.
                a href="#main"
                  class="sr-only focus:not-sr-only focus:absolute focus:left-4 focus:top-3 focus:z-50 focus:rounded-md focus:px-3 focus:py-2 focus:text-sm focus:font-semibold" {
                    (i18n::t("Skip to content"))
                }
                header class="border-b border-hairline bg-paper" {
                    div class="mx-auto flex max-w-6xl flex-wrap items-center gap-x-6 gap-y-3 px-4 py-3 sm:px-6" {
                        a href="/" class="inline-flex shrink-0 items-center gap-2.5 text-[21px] font-extrabold tracking-tight text-ink" {
                            (mark())
                            (SITE_NAME)
                        }
                        // Search sits in the header on every page; on a phone it
                        // drops to its own row under the logo and the links.
                        form method="get" action="/search" role="search"
                            class="relative order-last w-full sm:order-none sm:w-auto sm:min-w-[16rem] sm:max-w-md sm:flex-1" {
                            input type="search" name="q" autocomplete="off"
                                aria-label=(i18n::t("Search"))
                                placeholder=(i18n::t("Search people and parties..."))
                                class="h-11 w-full rounded-[10px] border border-hairline-strong bg-paper pl-10 pr-3 text-base text-ink placeholder:text-ink-faint";
                            svg aria-hidden="true" focusable="false" width="18" height="18" viewBox="0 0 24 24" fill="none"
                                stroke="currentColor" stroke-width="2" stroke-linecap="round"
                                class="pointer-events-none absolute left-3.5 top-3.5 text-ink-muted" {
                                circle cx="11" cy="11" r="7" {}
                                path d="M20 20l-3.5-3.5" {}
                            }
                        }
                        nav class="ml-auto flex items-center gap-3 text-[15px] sm:gap-4" aria-label=(i18n::t("Account")) {
                            // Language switcher: a native dropdown (no JavaScript).
                            details class="group relative" {
                                summary class="flex cursor-pointer list-none items-center gap-1.5 rounded-md px-1 py-1 text-ink-muted transition-colors hover:text-ink [&::-webkit-details-marker]:hidden" {
                                    (i18n::current().name())
                                    svg aria-hidden="true" focusable="false" width="12" height="12" viewBox="0 0 24 24" fill="none"
                                        stroke="currentColor" stroke-width="2.5" stroke-linecap="round"
                                        class="transition-transform group-open:rotate-180" {
                                        path d="M6 9l6 6 6-6" {}
                                    }
                                }
                                div class="absolute right-0 z-30 mt-2 min-w-[10rem] overflow-hidden rounded-[10px] border border-hairline bg-paper py-1 shadow-sm" {
                                    @for lang in i18n::Lang::ALL {
                                        a href={"/lang/" (lang.code())} lang=(lang.html_tag())
                                          aria-current=[lang.is_active().then_some("true")]
                                          class=(if lang.is_active() {
                                              "flex items-center justify-between gap-3 px-3.5 py-2 font-semibold text-ink"
                                          } else {
                                              "flex items-center justify-between gap-3 px-3.5 py-2 text-ink-muted transition-colors hover:bg-paper-sunken hover:text-ink"
                                          }) {
                                            span { (lang.name()) }
                                            span class="text-xs text-ink-faint" { (lang.label()) }
                                        }
                                    }
                                }
                            }
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
                                   class="inline-flex min-h-10 items-center rounded-lg bg-ink px-4 font-semibold text-paper transition-colors hover:bg-accent-strong" {
                                    (i18n::t("Register"))
                                }
                            }
                        }
                    }
                }
                main id="main" class="mx-auto w-full max-w-6xl grow px-4 py-8 sm:px-6 sm:py-10" {
                    (content)
                }
                footer class="mt-16 border-t border-hairline bg-paper-sunken" {
                    div class="mx-auto flex max-w-6xl flex-wrap items-center gap-x-7 gap-y-2 px-4 py-7 text-[15px] text-ink-muted sm:px-6" {
                        span class="font-extrabold text-ink" { (SITE_NAME) }
                        span { (i18n::t("Open political data.")) }
                        nav class="flex flex-wrap gap-x-5 gap-y-1" aria-label=(i18n::t("Site")) {
                            a href="/participation" class="op-link" { (i18n::t("Participation")) }
                            a href="/privacy" class="op-link" { (i18n::t("Privacy")) }
                            a href="https://github.com/markedown/open-public" class="op-link" { (i18n::t("Source code")) }
                        }
                        span class="sm:ml-auto" { (i18n::t("Data free to reuse (CC0)")) }
                    }
                }
            }
        }
    }
}

/// The logo mark: six seats in two rows, three filled. It reads as a small
/// chamber without claiming any party's colour.
pub fn mark() -> Markup {
    html! {
        span aria-hidden="true" class="inline-grid grid-cols-3 gap-[2px]" {
            @for filled in [true, true, false, true, false, false] {
                span class=(if filled {
                    "size-[6px] rounded-full bg-ink"
                } else {
                    "size-[6px] rounded-full bg-hairline-strong"
                }) {}
            }
        }
    }
}
