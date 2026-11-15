//! The command a browser's machine forwards the way in with, for the Remote access section and
//! `octoboardd address`: `ssh -L` from a loopback port there to the owner socket ([`to_socket`])
//! or, on Windows, to the browser port ([`to_port`]), the login being the account the daemon runs
//! as on a `<server>` the owner names.
//!
//! `ExitOnForwardFailure` makes `ssh` give up rather than run on when something else already holds
//! the local port, which would then be what the browser reaches; the forward binds `127.0.0.1`
//! alone, since with no address `ssh` binds `::1` too and counts a forward that got only one of the
//! two as made, the other being someone else's.

use std::path::Path;

/// The forward of the socket at `socket` to the local `port`, the login being `user`'s.
pub fn to_socket(port: u16, socket: &Path, user: Option<&str>) -> String {
    format!(
        "ssh -N -o ExitOnForwardFailure=yes -L 127.0.0.1:{port}:{} {}",
        shell_quoted(&socket.to_string_lossy()),
        login(user),
    )
}

/// [`to_socket`] for the owner socket of the data directory `data`, the login being the account
/// the daemon runs as.
pub fn to_socket_in(port: u16, data: &Path) -> String {
    to_socket(
        port,
        &super::socket::path_in(data),
        account_name().as_deref(),
    )
}

/// The forward of the browser port `port` to the same port on the browser's machine, the login
/// being `user`'s. The remote end is `127.0.0.1` rather than `localhost`, which the server could
/// resolve to `::1`, where nothing listens.
pub fn to_port(port: u16, user: Option<&str>) -> String {
    format!(
        "ssh -N -o ExitOnForwardFailure=yes -L 127.0.0.1:{port}:127.0.0.1:{port} {}",
        login(user)
    )
}

/// [`to_port`], the login being the account the daemon runs as.
pub fn to_port_for_account(port: u16) -> String {
    to_port(port, account_name().as_deref())
}

/// The account the daemon runs as, which `USER` need not name (under `su` or `sudo`); `None` when
/// it cannot be told.
fn account_name() -> Option<String> {
    crate::platform::fs::account_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|user| !user.is_empty())
}

/// The login on the `<server>` the owner names: `user`'s, quoted for the shell, or the server
/// alone.
fn login(user: Option<&str>) -> String {
    match user {
        Some(user) => format!("{}@<server>", shell_quoted(user)),
        None => "<server>".to_owned(),
    }
}

/// `text` as one word for a POSIX shell: as it is when it holds nothing the shell would read,
/// else in single quotes.
pub fn shell_quoted(text: &str) -> String {
    let plain = !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/._-+:@%,=".contains(c));
    if plain {
        text.to_owned()
    } else {
        format!("'{}'", text.replace('\'', r"'\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_port_forward_names_the_port_on_both_ends_and_the_login() {
        assert_eq!(
            to_port(3369, Some("alice")),
            "ssh -N -o ExitOnForwardFailure=yes -L 127.0.0.1:3369:127.0.0.1:3369 alice@<server>"
        );
        assert_eq!(
            to_port(4000, None),
            "ssh -N -o ExitOnForwardFailure=yes -L 127.0.0.1:4000:127.0.0.1:4000 <server>"
        );
        assert_eq!(
            to_port(4000, Some("Jane Doe")),
            "ssh -N -o ExitOnForwardFailure=yes -L 127.0.0.1:4000:127.0.0.1:4000 'Jane Doe'@<server>"
        );
    }

    /// The socket's forward quotes the path and the login alike, where the shell would read them.
    #[test]
    fn the_socket_forward_quotes_the_path_and_the_login() {
        assert_eq!(
            to_socket(
                3369,
                Path::new("/home/a/.octoboard/browser.sock"),
                Some("alice")
            ),
            "ssh -N -o ExitOnForwardFailure=yes -L 127.0.0.1:3369:/home/a/.octoboard/browser.sock \
             alice@<server>"
        );
        assert_eq!(
            to_socket(3369, Path::new("/Users/Jane Doe/data/browser.sock"), Some("Jane Doe")),
            "ssh -N -o ExitOnForwardFailure=yes -L 127.0.0.1:3369:'/Users/Jane Doe/data/browser.sock' \
             'Jane Doe'@<server>"
        );
        assert_eq!(
            to_socket(3369, Path::new("/s"), None),
            "ssh -N -o ExitOnForwardFailure=yes -L 127.0.0.1:3369:/s <server>"
        );
    }

    /// A word is handed to the shell as it is only when the shell would read nothing in it.
    #[test]
    fn a_word_the_shell_would_read_is_quoted() {
        assert_eq!(
            shell_quoted("/home/a/.octoboard/browser.sock"),
            "/home/a/.octoboard/browser.sock"
        );
        assert_eq!(shell_quoted(""), "''");
        assert_eq!(shell_quoted("a b"), "'a b'");
        assert_eq!(shell_quoted("$HOME/x"), "'$HOME/x'");
        assert_eq!(shell_quoted("a;b"), "'a;b'");
        assert_eq!(shell_quoted("~/x"), "'~/x'");
        assert_eq!(shell_quoted("it's"), r"'it'\''s'");
    }
}
