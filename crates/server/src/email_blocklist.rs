//! The vendored disposable-email-domain blocklist.
//!
//! Registration refuses a throwaway-inbox domain, so one actor cannot mint
//! unlimited verified accounts from a service that hands out disposable
//! addresses. The list is CC0, vendored (never fetched at runtime), and matched
//! on the exact lowercased domain. It is a cost-raiser, not proof of a real
//! person: the list is never complete and a determined actor runs their own
//! domain. Refresh it with `scripts/update_disposable_domains.sh`.

use std::collections::HashSet;
use std::sync::OnceLock;

const RAW: &str = include_str!("../data/disposable_email_domains.txt");

fn set() -> &'static HashSet<&'static str> {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| {
        RAW.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .collect()
    })
}

/// Whether an email's domain is a known disposable-inbox provider. The domain is
/// lowercased and matched exactly against the vendored list; a value with no `@`
/// is not disposable (it is rejected earlier as an invalid address).
pub fn is_disposable_email(email: &str) -> bool {
    let Some((_, domain)) = email.rsplit_once('@') else {
        return false;
    };
    let domain = domain.trim().to_lowercase();
    set().contains(domain.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_disposable_domains_are_flagged() {
        assert!(is_disposable_email("someone@mailinator.com"));
        // Case-insensitive on the domain.
        assert!(is_disposable_email("Someone@Mailinator.COM"));
    }

    #[test]
    fn ordinary_domains_are_not_flagged() {
        assert!(!is_disposable_email("someone@gmail.com"));
        assert!(!is_disposable_email("someone@example.com"));
        // No address shape: not disposable (rejected as invalid elsewhere).
        assert!(!is_disposable_email("not-an-email"));
    }

    #[test]
    fn the_list_loaded_and_is_substantial() {
        // A sanity floor so a broken/empty data file fails loudly.
        assert!(set().len() > 1000);
    }
}
