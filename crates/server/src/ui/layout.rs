use maud::{html, Markup, DOCTYPE};

use crate::i18n::{self, SITE_NAME};

pub fn document(
    page_title: Option<&str>,
    logged_in: bool,
    is_admin: bool,
    content: Markup,
) -> Markup {
    let title = match page_title {
        Some(t) => format!("{t} · {SITE_NAME}"),
        None => SITE_NAME.to_string(),
    };
    html! {
        (DOCTYPE)
        html lang=(i18n::lang_code()) {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (title) }
                link rel="stylesheet" href="/static/app.css";
                link rel="icon" href="/static/brand/favicon.svg" type="image/svg+xml";
                link rel="icon" href="/static/brand/favicon-32.png" sizes="32x32" type="image/png";
                link rel="icon" href="/static/brand/favicon-16.png" sizes="16x16" type="image/png";
                link rel="apple-touch-icon" href="/static/brand/apple-touch-icon.png";
                script src="/static/htmx.min.js" defer {}
            }
            body class="flex min-h-screen flex-col bg-paper font-sans text-ink antialiased" {
                // A thin accent strip at the very top: the one interface colour,
                // used as a quiet brand signature.
                div class="h-1 bg-accent" {}
                header class="sticky top-0 z-20 border-b border-hairline bg-paper" {
                    nav class="mx-auto flex h-16 max-w-5xl items-center justify-between px-4 sm:px-6" {
                        a href="/" class="inline-flex shrink-0 font-serif text-[19px] font-semibold tracking-[0.01em] text-ink" {
                            (SITE_NAME)
                        }
                        div class="flex items-center gap-4 text-[13px] font-medium sm:gap-5" {
                            // Language switcher: a native dropdown (no JavaScript),
                            // so it scales as more languages are added. Selecting
                            // one sets a cookie and returns to the current page.
                            details class="group relative" {
                                summary class="flex cursor-pointer list-none items-center gap-1 font-mono text-[11px] font-bold text-ink-muted transition-colors hover:text-accent [&::-webkit-details-marker]:hidden" {
                                    (i18n::current().label())
                                    span class="text-[8px] leading-none transition-transform group-open:rotate-180" { "▼" }
                                }
                                div class="absolute right-0 z-30 mt-2 min-w-[9rem] border border-hairline bg-paper py-1 shadow-sm" {
                                    @for lang in i18n::Lang::ALL {
                                        a href={"/lang/" (lang.code())}
                                          class=(if lang.is_active() {
                                              "flex items-center justify-between gap-3 px-3 py-1.5 text-[12px] font-medium text-ink"
                                          } else {
                                              "flex items-center justify-between gap-3 px-3 py-1.5 text-[12px] font-medium text-ink-muted transition-colors hover:bg-paper-raised hover:text-accent"
                                          }) {
                                            span { (lang.name()) }
                                            span class="font-mono text-[10px] font-bold" { (lang.label()) }
                                        }
                                    }
                                }
                            }
                            // People and parties are reached through a country or
                            // through search, never as a global cross-country list.
                            a href="/search" class="text-ink-muted transition-colors hover:text-accent" {
                                (i18n::t("Search"))
                            }
                            @if is_admin {
                                a href="/admin" class="text-ink-muted transition-colors hover:text-accent" {
                                    (i18n::t("Admin panel"))
                                }
                            }
                            @if logged_in {
                                a href="/submissions" class="text-ink-muted transition-colors hover:text-accent" {
                                    (i18n::t("My submissions"))
                                }
                                form method="post" action="/logout" {
                                    button type="submit"
                                        class="border border-ink bg-paper px-3 py-1.5 text-[11px] font-bold uppercase tracking-wide text-ink transition-colors hover:border-accent hover:text-accent" {
                                        (i18n::t("Log out"))
                                    }
                                }
                            } @else {
                                a href="/login" class="text-ink-muted transition-colors hover:text-accent" {
                                    (i18n::t("Log in"))
                                }
                                a href="/register"
                                   class="border border-ink bg-ink px-4 py-1.5 text-[11px] font-bold uppercase tracking-wide text-paper transition-colors hover:border-accent hover:bg-accent" {
                                    (i18n::t("Register"))
                                }
                            }
                        }
                    }
                }
                main class="mx-auto w-full max-w-5xl grow px-4 py-10 sm:px-6 sm:py-14" {
                    (content)
                }
                footer class="mt-16 border-t border-hairline" {
                    div class="mx-auto flex max-w-5xl items-center justify-between px-4 py-6 text-xs font-medium text-ink-muted sm:px-6" {
                        span { (i18n::t("Open political data.")) }
                        nav class="flex gap-4" {
                            a href="/search" class="transition-colors hover:text-accent" { (i18n::t("Search")) }
                        }
                    }
                }
            }
        }
    }
}
