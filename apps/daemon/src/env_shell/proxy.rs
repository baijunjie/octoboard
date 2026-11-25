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
    proxy_in(shell_env, host, crate::platform::shell::ENV_NAMES_FOLD_CASE)
}

/// The proxy `shell_env` names for a request to `url`, matched by the host it names
/// ([`host_of`]). The error says which variable is not a proxy address, or that `url` names no
/// host.
pub fn proxy_for_url(
    shell_env: &HashMap<String, String>,
    url: &str,
) -> Result<Option<ureq::Proxy>, String> {
    let host = host_of(url).ok_or_else(|| format!("{url} names no host"))?;
    proxy_for(shell_env, host)
}

/// The host an `https://` or `http://` `url` names, which `NO_PROXY` is matched against: what
/// comes before the first `/`, `?` or `#`, without user information or a port.
pub fn host_of(url: &str) -> Option<&str> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority.rsplit('@').next()?;
    let host = host.split(':').next()?;
    (!host.is_empty()).then_some(host)
}

/// [`proxy_for`], with variable names matched without regard to case where `fold_case`.
fn proxy_in(
    shell_env: &HashMap<String, String>,
    host: &str,
    fold_case: bool,
) -> Result<Option<ureq::Proxy>, String> {
    // The exact spelling first; failing that, of the spellings that differ only in case, the
    // least, so an environment that holds several (a snapshot can, a Windows process cannot)
    // resolves the same way every time, whatever order the map keeps them in.
    let value = |name: &str| {
        shell_env
            .get(name)
            .or_else(|| {
                fold_case
                    .then(|| {
                        shell_env
                            .iter()
                            .filter(|(key, _)| key.eq_ignore_ascii_case(name))
                            .min_by_key(|(key, _)| key.as_str())
                            .map(|(_, value)| value)
                    })
                    .flatten()
            })
            .map(|value| value.trim())
    };
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

    /// Where names fold case, a variable is found however it is spelled, `NO_PROXY` too; where
    /// they do not, only the two spellings tools read count.
    #[test]
    fn a_variable_spelled_in_mixed_case_counts_only_where_names_fold_case() {
        let mixed = env(&[("Https_Proxy", "http://secure:2")]);
        let host = |fold| {
            proxy_in(&mixed, HOST, fold)
                .unwrap()
                .map(|p| p.host().to_owned())
        };
        assert_eq!(host(true), Some("secure".to_owned()));
        assert_eq!(host(false), None);
        let exempt = env(&[("HTTPS_PROXY", "http://secure:2"), ("No_Proxy", HOST)]);
        assert!(proxy_in(&exempt, HOST, true).unwrap().is_none());
        assert!(proxy_in(&exempt, HOST, false).unwrap().is_some());
    }

    /// Folding case changes which spelling is found, not which variable comes first: HTTPS, then
    /// ALL, then HTTP, and the exact spellings still count.
    #[test]
    fn the_variables_apply_in_the_same_order_however_they_are_spelled_where_names_fold_case() {
        let host = |pairs: &[(&str, &str)]| {
            proxy_in(&env(pairs), HOST, true)
                .unwrap()
                .map(|p| p.host().to_owned())
        };
        assert_eq!(
            host(&[
                ("Http_Proxy", "http://plain:1"),
                ("All_Proxy", "http://all:3"),
                ("Https_Proxy", "http://secure:2"),
            ]),
            Some("secure".to_owned())
        );
        assert_eq!(
            host(&[
                ("Http_Proxy", "http://plain:1"),
                ("All_Proxy", "http://all:3"),
            ]),
            Some("all".to_owned())
        );
        assert_eq!(
            host(&[("http_proxy", "http://plain:1")]),
            Some("plain".to_owned())
        );
        assert_eq!(
            host(&[("HTTPS_PROXY", "http://secure:2")]),
            Some("secure".to_owned())
        );
        assert_eq!(host(&[("HTTPS_PROXY_EXTRA", "http://other:4")]), None);
    }

    /// Where names fold case and several spellings of one name are there, the exact spelling
    /// wins, and without it the least of the others, whatever order the map keeps them in.
    #[test]
    fn several_spellings_of_one_name_resolve_the_same_way_every_time_where_names_fold_case() {
        let both = env(&[
            ("https_proxy", "http://lower:1"),
            ("Https_Proxy", "http://mixed:3"),
            ("HTTPS_PROXY", "http://upper:2"),
        ]);
        for _ in 0..8 {
            let host = proxy_in(&both, HOST, true)
                .unwrap()
                .map(|p| p.host().to_owned());
            assert_eq!(host, Some("upper".to_owned()));
        }
        for _ in 0..8 {
            let inexact = env(&[
                ("https_proxy", "http://lower:1"),
                ("Https_Proxy", "http://mixed:3"),
                ("hTTPS_PROXY", "http://odd:4"),
            ]);
            let host = proxy_in(&inexact, HOST, true)
                .unwrap()
                .map(|p| p.host().to_owned());
            assert_eq!(host, Some("mixed".to_owned()), "`Https_Proxy` sorts first");
        }
    }

    /// Windows names its variables without regard to case, and so does the daemon there.
    #[cfg(windows)]
    #[test]
    fn on_windows_a_proxy_variable_counts_whatever_its_case() {
        assert_eq!(
            proxy_host(&[("Https_Proxy", "http://secure:2")], HOST),
            Some("secure".to_owned())
        );
    }

    #[test]
    fn a_value_that_is_no_proxy_address_names_its_variable() {
        let err = proxy_for(&env(&[("ALL_PROXY", "://")]), HOST).unwrap_err();
        assert_eq!(err, "ALL_PROXY is not a proxy address");
    }

    #[test]
    fn a_manifest_s_host_is_what_its_url_names() {
        assert_eq!(
            host_of("https://updater.example.com/manifests/darwin_arm64.json"),
            Some("updater.example.com")
        );
        assert_eq!(
            host_of("http://user@127.0.0.1:8080/m.json"),
            Some("127.0.0.1")
        );
        assert_eq!(host_of("https://"), None);
        assert_eq!(host_of("ftp://example.com/m.json"), None);
    }
}
