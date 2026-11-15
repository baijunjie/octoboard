//! Opening a remote daemon's own UI in the system's browser, from the shell: the address carries
//! the owner key, which no page of the application is ever handed (no remote-daemons command
//! returns a secret), so the shell opens it itself (`open_address`, which never repeats the
//! address).

/// Opens `url`, an `http:` address on a loopback port, in the user's default browser.
pub fn open(url: &str) -> Result<(), String> {
    crate::open_address::open(url).map_err(|err| format!("cannot open the browser: {err}"))
}
