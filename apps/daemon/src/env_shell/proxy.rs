//! The proxy the user's shell environment names for a request to a host, which the daemon's own
//! HTTP requests (the npm registry, the release manifest) go through the way the user's network
//! reaches them. Never the daemon's own environment, which is not the user's network.

use std::collections::HashMap;

/// The environment variables that name the proxy for an HTTPS request, in the order they apply.
const PROXY_VARIABLES: [&str; 6] = [
    "HTTPS_PROXY",
    "https_proxy",
    "ALL_PROXY",
    "all_proxy",
    "HTTP_PROXY",
    "http_proxy",
];

/// The proxy `shell_env` names for `host`, `None` when it names none or `NO_PROXY` exempts `host`.
/// The error says which variable is not a proxy address.
pub fn proxy_for(
    shell_env: &HashMap<String, String>,
    host: &str,
) -> Result<Option<ureq::Proxy>, String> {
    let value = |name: &str| shell_env.get(name).map(|value| value.trim());
    let no_proxy = value("NO_PROXY")
        .or_else(|| value("no_proxy"))
        .unwrap_or("");
    if bypasses(no_proxy, host) {
        return Ok(None);
    }
    for name in PROXY_VARIABLES {
        let Some(url) = value(name).filter(|url| !url.is_empty()) else {
            continue;
        };
        return ureq::Proxy::new(url)
            .map(Some)
            .map_err(|_| format!("{name} is not a proxy address"));
    }
    Ok(None)
}

/// Whether the `NO_PROXY` list `no_proxy` names `host`: `*`, the host itself, or a domain it is
/// under, written with or without a leading `.` or `*.`.
fn bypasses(no_proxy: &str, host: &str) -> bool {
    no_proxy.split(',').map(str::trim).any(|entry| {
        let domain = entry.trim_start_matches('*').trim_start_matches('.');
        entry == "*"
            || (!domain.is_empty()
                && (host.eq_ignore_ascii_case(domain)
                    || host
                        .to_ascii_lowercase()
                        .ends_with(&format!(".{}", domain.to_ascii_lowercase()))))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOST: &str = "github.com";

    fn env(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect()
    }

    fn proxy_host(pairs: &[(&str, &str)], host: &str) -> Option<String> {
        proxy_for(&env(pairs), host)
            .unwrap()
            .map(|proxy| proxy.host().to_owned())
    }

    #[test]
    fn no_proxy_variable_means_no_proxy() {
        assert_eq!(proxy_host(&[], HOST), None);
        assert_eq!(proxy_host(&[("HTTPS_PROXY", "  ")], HOST), None);
    }

    #[test]
    fn the_variables_apply_in_order_of_https_all_then_http() {
        assert_eq!(
            proxy_host(
                &[
                    ("http_proxy", "http://plain:1"),
                    ("HTTPS_PROXY", "http://secure:2")
                ],
                HOST
            ),
            Some("secure".to_owned())
        );
        assert_eq!(
            proxy_host(
                &[
                    ("HTTP_PROXY", "http://plain:1"),
                    ("all_proxy", "http://all:3")
                ],
                HOST
            ),
            Some("all".to_owned())
        );
        assert_eq!(
            proxy_host(&[("http_proxy", "http://plain:1")], HOST),
            Some("plain".to_owned())
        );
    }

    #[test]
    fn no_proxy_exempts_the_host_it_names_by_name_or_by_domain_for_any_host() {
        let with = |no_proxy: &str, host: &str| {
            proxy_host(
                &[("HTTPS_PROXY", "http://secure:2"), ("NO_PROXY", no_proxy)],
                host,
            )
        };
        assert_eq!(with("*", HOST), None);
        assert_eq!(with("github.com", HOST), None);
        assert_eq!(with("GitHub.com", HOST), None);
        assert_eq!(with("localhost, .github.com", "objects.github.com"), None);
        assert_eq!(with("*.github.com", "objects.github.com"), None);
        // A domain does not exempt a host that merely ends in the same letters, nor another host.
        assert!(with("hub.com", HOST).is_some());
        assert!(with("internal.example", HOST).is_some());
        // The lower-case spelling counts too.
        assert_eq!(
            proxy_host(
                &[("HTTPS_PROXY", "http://secure:2"), ("no_proxy", HOST)],
                HOST
            ),
            None
        );
    }

    #[test]
    fn a_value_that_is_no_proxy_address_names_its_variable() {
        let err = proxy_for(&env(&[("ALL_PROXY", "://")]), HOST).unwrap_err();
        assert_eq!(err, "ALL_PROXY is not a proxy address");
    }
}
