//! What a remote URL says: whether it carries a credential, how it reads without one, and whether
//! a clone may be made from it.

/// Splits a `scheme://userinfo@host...` URL into its scheme, its userinfo and what follows the
/// `@`. `None` for a URL with no userinfo, or of another shape such as `git@host:path`.
fn split_userinfo(url: &str) -> Option<(&str, &str, &str)> {
    let (scheme, rest) = url.split_once("://")?;
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (userinfo, _) = rest[..authority_end].rsplit_once('@')?;
    Some((scheme, userinfo, &rest[userinfo.len() + 1..]))
}

/// Whether the userinfo of a `scheme://` URL is a credential: any of it for `http(s)` and `git`,
/// where the user part is routinely a token, and a password for the `ssh` family, whose plain user
/// (`ssh://git@host/...`) is how the remote is named, not a secret.
fn userinfo_is_credential(scheme: &str, userinfo: &str) -> bool {
    if scheme.to_ascii_lowercase().contains("ssh") {
        userinfo.contains(':')
    } else {
        true
    }
}

/// Whether `url` carries a credential in its userinfo, such as `https://user:token@host/repo`.
pub fn has_credentials(url: &str) -> bool {
    split_userinfo(url)
        .is_some_and(|(scheme, userinfo, _)| userinfo_is_credential(scheme, userinfo))
}

/// `url` with any credential in its userinfo removed, for showing a remote without its secret.
pub fn without_credentials(url: &str) -> String {
    let Some((scheme, userinfo, rest)) = split_userinfo(url) else {
        return url.to_string();
    };
    if !userinfo_is_credential(scheme, userinfo) {
        return url.to_string();
    }
    if scheme.to_ascii_lowercase().contains("ssh") {
        let user = userinfo.split(':').next().unwrap_or_default();
        return format!("{scheme}://{user}@{rest}");
    }
    format!("{scheme}://{rest}")
}

/// Whether `url` is one a clone may be made from: an `http(s)`, `ssh` (also as `git+ssh` and
/// `ssh+git`) or `git` URL, or the `user@host:path` shape, with a host that cannot read as an
/// option and no whitespace or control character anywhere in it. A local path, `file://`, the
/// helper transports (`ext::`) and the obsolete `ftp` are not: a clone from a document's URL must
/// reach a remote and never run a command or read a path of this machine. That is stricter than
/// adding a project by hand, which leaves the choice to git. Clones that a document or a tool call
/// asks for are held to it.
pub fn is_clonable_url(url: &str) -> bool {
    if url.is_empty() || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return false;
    }
    let lowered = url.to_ascii_lowercase();
    for scheme in [
        "https://",
        "http://",
        "ssh://",
        "git://",
        "git+ssh://",
        "ssh+git://",
    ] {
        if let Some(rest) = lowered.strip_prefix(scheme) {
            let authority = rest.split('/').next().unwrap_or("");
            return host_is_usable(authority);
        }
    }
    // `[user@]host:path`: no `/` before the first `:`, and no `:` or `//` right after it (`ext::`,
    // `file://`).
    let Some((host, path)) = url.split_once(':') else {
        return false;
    };
    host_is_usable(host)
        && !host.contains('/')
        && !path.is_empty()
        && !path.starts_with(':')
        && !path.starts_with("//")
}

/// Whether `authority` (`[user@]host[:port]`, the host being what follows the last `@`) has a host
/// and cannot be read as an option by `ssh` or `git`, whole or by its host.
fn host_is_usable(authority: &str) -> bool {
    let host = authority.rsplit('@').next().unwrap_or("");
    !host.is_empty() && !authority.starts_with('-') && !host.starts_with('-')
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A credential in the userinfo of a `scheme://` URL is told apart from the user an `ssh`
    /// remote is named by, and removed from what is shown; a URL without one is left as it is.
    #[test]
    fn credentials_in_a_remote_url_are_found_and_removed() {
        for (url, shown) in [
            (
                "https://user:token@example.com/r.git",
                "https://example.com/r.git",
            ),
            (
                "https://token@example.com/r.git",
                "https://example.com/r.git",
            ),
            (
                "ssh://git:secret@example.com:22/r.git",
                "ssh://git@example.com:22/r.git",
            ),
        ] {
            assert!(has_credentials(url), "{url}");
            assert_eq!(without_credentials(url), shown);
        }
        for url in [
            "https://example.com/a@b.git",
            "ssh://git@example.com/r.git",
            "git@example.com:owner/r.git",
            "/srv/repo",
        ] {
            assert!(!has_credentials(url), "{url}");
            assert_eq!(without_credentials(url), url);
        }
    }
    #[test]
    fn only_a_url_that_reaches_a_remote_may_be_cloned() {
        for url in [
            "https://github.com/o/r.git",
            "HTTP://example.com/r",
            "ssh://git@example.com/o/r.git",
            "git://example.com/r.git",
            "git+ssh://git@example.com/o/r.git",
            "ssh+git://example.com/o/r.git",
            "ssh://user@host:2222/o/r.git",
            "git@github.com:o/r.git",
            "host:path/r",
        ] {
            assert!(is_clonable_url(url), "{url}");
        }
        for url in [
            "",
            "/srv/git/r",
            "file:///srv/git/r",
            "ext::sh -c touch% /tmp/x",
            "ext::sh",
            "fd::3",
            "--upload-pack=touch",
            "-oProxyCommand=x:y",
            "https://",
            "https:///r",
            "ssh://-oProxyCommand=x/r",
            "https://example.com/r\n--upload-pack=x",
            "https://example.com/a b",
            "r.git",
            ":path",
            "host:",
            "a/b:c",
            "ftp://example.com/r.git",
            "ftps://example.com/r.git",
            // A leading `-` hidden behind the user part.
            "ssh://user@-host/r",
            "ssh://user@/r",
            "user@-host:path/r",
            "-user@host:path/r",
        ] {
            assert!(!is_clonable_url(url), "{url:?}");
        }
    }
}
