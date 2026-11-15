//! The words the one menu bar icon names daemons by: the short name a remote daemon's tab is
//! labelled with, and the accessible name (the tooltip on Windows), which names the daemon whose
//! state the icon shows after the remote daemon's name.

/// The longest short name, in characters, before it is cut with an ellipsis: a tab is one of a row
/// that has to fit the menu's width.
const SHORT_NAME_MAX: usize = 12;

/// The label of a remote daemon's tab: a host name's first label (`build` for
/// `build.example.com`), an IP address or any other name whole, either cut to `SHORT_NAME_MAX`
/// characters.
pub(super) fn short_name(name: &str) -> String {
    let name = name.trim();
    let short = if name.parse::<std::net::IpAddr>().is_ok() || !is_host_name(name) {
        name
    } else {
        name.split('.').next().unwrap_or(name)
    };
    if short.chars().count() <= SHORT_NAME_MAX {
        return short.to_owned();
    }
    let mut cut: String = short.chars().take(SHORT_NAME_MAX - 1).collect();
    cut.push('…');
    cut
}

/// Whether `name` reads as a dotted host name: labels of letters, digits and hyphens, the first one
/// not empty.
fn is_host_name(name: &str) -> bool {
    name.contains('.')
        && !name.starts_with('.')
        && name
            .split('.')
            .all(|label| label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
}

/// The icon's accessible name (its tooltip on Windows): `label` alone while it shows the local
/// daemon's state, and after the daemon's name while it shows a remote daemon's, as its window's
/// title names it.
pub(super) fn named(remote: Option<&str>, label: &str) -> String {
    match remote {
        Some(name) => format!("{name} — {label}"),
        None => label.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_name_is_shortened_to_its_first_label() {
        assert_eq!(short_name("build.example.com"), "build");
        assert_eq!(short_name(" pi.local "), "pi");
    }

    #[test]
    fn an_address_or_a_name_of_the_owner_s_own_is_kept_whole() {
        assert_eq!(short_name("192.168.1.20"), "192.168.1.20");
        assert_eq!(short_name("::1"), "::1");
        assert_eq!(short_name("Home server"), "Home server");
        assert_eq!(short_name("v1.2 box"), "v1.2 box");
        assert_eq!(short_name(".hidden"), ".hidden");
    }

    #[test]
    fn a_long_short_name_is_cut_with_an_ellipsis() {
        assert_eq!(short_name("the-build-machine.lan"), "the-build-m…");
        assert_eq!(
            short_name("我的家用服务器在客厅里面的角落"),
            "我的家用服务器在客厅里…"
        );
        assert_eq!(short_name("exactly12chr"), "exactly12chr");
    }

    #[test]
    fn a_remote_icon_is_named_after_its_daemon() {
        assert_eq!(named(None, "Octoboard"), "Octoboard");
        assert_eq!(named(Some("build"), "Octoboard"), "build — Octoboard");
    }

    #[test]
    fn a_name_with_a_trailing_dot_or_spaces_around_it_is_still_a_host_name() {
        assert_eq!(short_name("build.example.com."), "build");
        assert_eq!(short_name("\tbuild.lan\n"), "build");
    }

    #[test]
    fn nothing_makes_a_short_name_longer_than_the_menu_bar_has_room_for() {
        for name in [
            "",
            "a",
            "a.b.c.d.e.f.g.h.i.j.k.l.m.n",
            "an-extraordinarily-long-first-label.example.com",
            "2001:db8:85a3:0:0:8a2e:370:7334",
            "我的非常非常非常长的服务器名称",
        ] {
            assert!(short_name(name).chars().count() <= 12, "{name}");
        }
        assert_eq!(short_name(""), "");
    }

    #[test]
    fn an_accessible_name_leads_with_the_daemon_s_whole_name_not_its_short_one() {
        assert_eq!(
            named(Some("build.example.com"), "Octoboard"),
            "build.example.com — Octoboard"
        );
    }
}
