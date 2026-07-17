pub mod background;
pub mod badge;
pub mod breadcrumb;
pub mod button;
pub mod citation;
pub mod election;
pub mod event;
pub mod layout;
pub mod news;
pub mod outlet;
pub mod pagination;
pub mod poll_widget;
pub mod references;
pub mod search;
pub mod seat_bar;
pub mod statement;
pub mod timeline_entry;
pub mod translated;

/// Up to two initials from a name's first and last word, for the initials
/// square shown on person rows.
pub fn initials(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().collect();
    let first = words.first().and_then(|w| w.chars().next());
    let last = words
        .last()
        .filter(|_| words.len() > 1)
        .and_then(|w| w.chars().next());
    first.into_iter().chain(last).collect()
}

/// The "record card" corner tick: two 14px accent brackets at the top-left and
/// bottom-right, sitting 1px outside the card's own border. The element it is
/// applied to becomes `relative`. Reserved for hero-level cards (Home country
/// cards, the Country/Party header block, the poll card) so it stays a
/// signature rather than decoration.
pub const CORNER_TICK: &str = "relative \
    before:absolute before:-left-px before:-top-px before:h-3.5 before:w-3.5 \
    before:border-l-2 before:border-t-2 before:border-accent before:content-[''] \
    after:absolute after:-bottom-px after:-right-px after:h-3.5 after:w-3.5 \
    after:border-b-2 after:border-r-2 after:border-accent after:content-['']";
