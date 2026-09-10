use std::sync::Arc;
use std::time::Duration;

use hickory_resolver::config::ResolverConfig;
use hickory_resolver::net::runtime::TokioRuntimeProvider;
use hickory_resolver::net::{DnsError, NetError};
use hickory_resolver::proto::op::ResponseCode;
use hickory_resolver::proto::rr::RData;
use hickory_resolver::{Resolver, TokioResolver};

/// Maximum duration allowed for DNS deliverability checks before failing open.
const DNS_TIMEOUT: Duration = Duration::from_secs(3);

/// Create the default email deliverability checker.
///
/// Attempts to use the host system's DNS configuration; falls back to default
/// public DNS configuration if system configuration fails.
/// If resolver construction fails completely, logs a warning and returns `AllowAll` (fail open).
pub fn default_deliverability() -> EmailDeliverability {
    let resolver = TokioResolver::builder_tokio()
        .and_then(|b| b.build())
        .or_else(|_| {
            Resolver::builder_with_config(
                ResolverConfig::default(),
                TokioRuntimeProvider::default(),
            )
            .build()
        });

    match resolver {
        Ok(r) => EmailDeliverability::Live(Arc::new(r)),
        Err(err) => {
            tracing::warn!(
                ?err,
                "failed to initialize DNS resolver; failing open with AllowAll"
            );
            EmailDeliverability::AllowAll
        }
    }
}

/// Determines whether an email's domain can accept mail.
#[derive(Clone)]
pub enum EmailDeliverability {
    /// Live asynchronous DNS resolver checking MX and A/AAAA records.
    Live(Arc<TokioResolver>),
    /// Always allows registration (used for offline development and fast test suites).
    AllowAll,
    /// Mock validator allowing programmatic domain control in tests.
    Mock(Arc<dyn Fn(&str) -> bool + Send + Sync>),
}

impl EmailDeliverability {
    /// Check whether an email domain is deliverable.
    ///
    /// Fails closed only on definitive evidence that the domain does not accept mail
    /// (NXDOMAIN, RFC 7505 Null MX, or no MX and no A/AAAA records).
    /// Fails open on transient DNS errors (timeouts, SERVFAIL, network issues).
    pub async fn is_deliverable(&self, email: &str) -> bool {
        match self {
            Self::AllowAll => true,
            Self::Mock(check) => check(email),
            Self::Live(resolver) => {
                let Some(domain) = extract_domain(email) else {
                    return false;
                };

                match tokio::time::timeout(DNS_TIMEOUT, check_domain(resolver, domain)).await {
                    Ok(deliverable) => deliverable,
                    Err(_) => {
                        tracing::warn!(domain, "DNS deliverability check timed out; failing open");
                        true
                    }
                }
            }
        }
    }
}

/// Extract the normalized domain portion of an email address.
pub fn extract_domain(email: &str) -> Option<&str> {
    let (_, domain) = email.rsplit_once('@')?;
    let domain = domain.trim().trim_end_matches('.');
    if domain.is_empty() || domain.len() > 253 {
        return None;
    }
    if !domain.contains('.') {
        return None;
    }
    for label in domain.split('.') {
        if label.is_empty() || label.len() > 63 {
            return None;
        }
    }
    Some(domain)
}

async fn check_domain(resolver: &TokioResolver, domain: &str) -> bool {
    // Append trailing dot to treat the email domain as an absolute FQDN,
    // avoiding local search domain expansion (e.g. domain.local / domain.fritz.box).
    let fqdn = format!("{domain}.");
    match resolver.mx_lookup(&fqdn).await {
        Ok(lookup) => {
            let answers = lookup.answers();
            let mut has_null_mx = false;
            let mut has_valid_mx = false;

            for record in answers {
                if let RData::MX(mx) = &record.data {
                    if mx.preference == 0 && mx.exchange.is_root() {
                        has_null_mx = true;
                    } else {
                        has_valid_mx = true;
                    }
                }
            }

            if has_null_mx && !has_valid_mx {
                tracing::info!(domain, "email domain rejected: RFC 7505 Null MX");
                false
            } else if has_valid_mx {
                true
            } else {
                check_ip_fallback(resolver, &fqdn).await
            }
        }
        Err(err) => match classify_error(&err) {
            DnsOutcome::NxDomainOrInvalid => {
                tracing::info!(
                    domain,
                    ?err,
                    "email domain rejected: non-existent or invalid domain"
                );
                false
            }
            DnsOutcome::NoRecords => check_ip_fallback(resolver, &fqdn).await,
            DnsOutcome::Transient => {
                tracing::warn!(
                    domain,
                    ?err,
                    "transient DNS error during MX lookup; failing open"
                );
                true
            }
        },
    }
}

async fn check_ip_fallback(resolver: &TokioResolver, fqdn: &str) -> bool {
    match resolver.lookup_ip(fqdn).await {
        Ok(ips) => {
            if ips.iter().next().is_some() {
                true
            } else {
                tracing::info!(fqdn, "email domain rejected: no MX and empty A/AAAA");
                false
            }
        }
        Err(err) => match classify_error(&err) {
            DnsOutcome::NxDomainOrInvalid | DnsOutcome::NoRecords => {
                tracing::info!(fqdn, ?err, "email domain rejected: no MX or A/AAAA records");
                false
            }
            DnsOutcome::Transient => {
                tracing::warn!(
                    fqdn,
                    ?err,
                    "transient DNS error during A/AAAA fallback; failing open"
                );
                true
            }
        },
    }
}

#[derive(Debug, PartialEq, Eq)]
enum DnsOutcome {
    NxDomainOrInvalid,
    NoRecords,
    Transient,
}

fn classify_error(err: &NetError) -> DnsOutcome {
    match err {
        NetError::Proto(_) => DnsOutcome::NxDomainOrInvalid,
        NetError::Dns(DnsError::ResponseCode(ResponseCode::NXDomain)) => {
            DnsOutcome::NxDomainOrInvalid
        }
        NetError::Dns(DnsError::NoRecordsFound(no_records)) => {
            if no_records.response_code == ResponseCode::NXDomain {
                DnsOutcome::NxDomainOrInvalid
            } else if no_records.response_code == ResponseCode::NoError {
                DnsOutcome::NoRecords
            } else {
                DnsOutcome::Transient
            }
        }
        _ => DnsOutcome::Transient,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_extraction() {
        assert_eq!(extract_domain("alice@example.com"), Some("example.com"));
        assert_eq!(
            extract_domain("alice.bob+tag@sub.domain.co.uk"),
            Some("sub.domain.co.uk")
        );
        assert_eq!(extract_domain("alice@example.com."), Some("example.com"));
        assert_eq!(
            extract_domain("alice@nested@domain.org"),
            Some("domain.org")
        );

        assert_eq!(extract_domain("no-at-sign.com"), None);
        assert_eq!(extract_domain("@"), None);
        assert_eq!(extract_domain("user@"), None);
        assert_eq!(extract_domain("user@nodot"), None);
        assert_eq!(extract_domain("user@.leading.dot"), None);
        assert_eq!(extract_domain("user@double..dot.com"), None);
        assert_eq!(extract_domain("user@ "), None);
    }

    #[tokio::test]
    async fn allow_all_accepts_any_domain() {
        let checker = EmailDeliverability::AllowAll;
        assert!(checker.is_deliverable("test@nonexistent.invalid").await);
        assert!(checker.is_deliverable("test@nodot").await);
    }

    #[tokio::test]
    async fn test_checker_allows_mocking() {
        let checker = EmailDeliverability::Mock(Arc::new(|email| email.ends_with("@allowed.com")));
        assert!(checker.is_deliverable("test@allowed.com").await);
        assert!(!checker.is_deliverable("test@rejected.com").await);
    }

    #[tokio::test]
    async fn live_resolver_deliverability() {
        let resolver = TokioResolver::builder_tokio()
            .and_then(|b| b.build())
            .expect("local system resolver");
        let checker = EmailDeliverability::Live(Arc::new(resolver));

        // Valid domain with MX
        assert!(checker.is_deliverable("user@gmail.com").await);

        // Valid domain without MX but with A/AAAA (RFC 5321 §5.1 fallback)
        assert!(checker.is_deliverable("user@www.google.com").await);

        // RFC 7505 Null MX domain (declares it accepts no email)
        assert!(!checker.is_deliverable("user@example.com").await);

        // Non-existent domain (NXDOMAIN)
        assert!(
            !checker
                .is_deliverable("user@definitely-non-existent-domain-9923847291.org")
                .await
        );

        // Malformed email
        assert!(!checker.is_deliverable("invalid-format").await);
        assert!(!checker.is_deliverable("user@nodot").await);
        assert!(!checker.is_deliverable("user@..invalid..").await);
        assert!(!checker.is_deliverable("user@-invalid-.com").await);
    }

    #[test]
    fn default_deliverability_constructs() {
        let deliverability = default_deliverability();
        assert!(matches!(
            deliverability,
            EmailDeliverability::Live(_) | EmailDeliverability::AllowAll
        ));
    }

    #[test]
    fn error_classification() {
        use hickory_resolver::net::DnsError;
        use hickory_resolver::proto::op::ResponseCode;

        let nxdomain = NetError::Dns(DnsError::ResponseCode(ResponseCode::NXDomain));
        assert_eq!(classify_error(&nxdomain), DnsOutcome::NxDomainOrInvalid);

        let proto = NetError::Proto(hickory_resolver::proto::ProtoError::from("bad"));
        assert_eq!(classify_error(&proto), DnsOutcome::NxDomainOrInvalid);

        let servfail = NetError::Dns(DnsError::ResponseCode(ResponseCode::ServFail));
        assert_eq!(classify_error(&servfail), DnsOutcome::Transient);

        let timeout = NetError::Timeout;
        assert_eq!(classify_error(&timeout), DnsOutcome::Transient);

        let busy = NetError::Busy;
        assert_eq!(classify_error(&busy), DnsOutcome::Transient);

        let io_err = NetError::Io(Arc::new(std::io::Error::new(
            std::io::ErrorKind::ConnectionReset,
            "reset",
        )));
        assert_eq!(classify_error(&io_err), DnsOutcome::Transient);
    }
}
