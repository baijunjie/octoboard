//! The MCP servers of a compatibility source (`adapter::CompatibilitySource`), translated into the
//! `-c mcp_servers.…` overrides of a Codex launch, so that Claude compatibility gives Codex the
//! source's servers. Which servers the source has is its own business; they arrive in its
//! declaration format (a JSON object per server, `${VAR}` expansion as Claude Code does it).
//!
//! Stdio servers and streamable HTTP servers (`type: "http"`) are translated; SSE and anything else
//! is skipped, as is a server that needs something Codex cannot express (`headersHelper`, an
//! environment variable whose name or value cannot be one), that names a variable which is not
//! set, or that would need to set a variable Octoboard or Codex itself relies on.
//!
//! Values that may be secrets stay off Codex's own argv, which anyone on the machine can list. A
//! header's value is put in the Codex process's environment under a name of Octoboard's own, and
//! `env_http_headers` names it. A stdio server's `env` entry is handed over by name with
//! `env_vars`, which Codex forwards to the server from its own environment; that needs the variable
//! of that name to hold the value, so it is set on the Codex process when the shell has no such
//! variable. An entry that would need a different value from the shell's (or from another server's)
//! cannot be passed that way, and its server is skipped rather than put on argv. So is a server
//! whose `env` names a variable the launch sets after the shell (`TERM`, `GIT_CONFIG_*`,
//! `OCTOBOARD_*`), whose final value is not known here, and one whose `env` or header value holds a
//! NUL byte. A variable that steers Codex itself (see [`Reserved`]) is passed only at the value the
//! launch or the shell already gives it.
//!
//! What is not kept off argv: a stdio server's expanded `command`, `args` and `cwd` are on Codex's
//! own argv (`-c mcp_servers.<name>.command`, `.args`, `.cwd`) for the session. That is accepted:
//! `command` and `args` become the argv of the server process itself when Codex starts it, exactly
//! as under Claude Code, so a `${VAR}` in them is expanded and passed on, and `cwd` is a path,
//! rarely a secret. The `url` of an HTTP server is on Codex's argv too, so a server whose `url`
//! takes a value from a variable, carries credentials (`user:pass@`) or has a query string (where a
//! token usually sits) is skipped.

use std::collections::{BTreeMap, HashMap};

use serde_json::Value;

use super::toml_string;

/// Prefix of the variables set on the Codex process to carry header values.
const HEADER_ENV_PREFIX: &str = "OCTOBOARD_CLAUDE_MCP_";

/// Ends the name of each such variable so that Codex's default filter, which drops the variables
/// whose names hold `TOKEN`, keeps them out of the commands the model runs.
const HEADER_ENV_SUFFIX: &str = "_TOKEN";

/// What a Codex launch adds for Claude Code's servers.
#[derive(Debug, Default, PartialEq)]
pub struct Servers {
    /// The values of `-c` flags, one `mcp_servers.<name>.<key>=<value>` each.
    pub overrides: Vec<String>,
    /// Variables to set on the Codex process.
    pub env: Vec<(String, String)>,
}

/// How a server's `env` may treat a variable name that the launch or Codex itself relies on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reserved {
    /// Not reserved: a server may set it.
    No,
    /// Codex reads it from its own environment, which the shell and `launch_env` fix: a server may
    /// name it only at the value it already holds.
    Held,
    /// Octoboard or the launch owns it, or sets it after the shell and `launch_env`, so its final
    /// value is not known here: a server naming it is skipped. `OCTOBOARD_*` is Octoboard's own
    /// namespace (its header variables are in `plan.env`), the session variables and `GIT_CONFIG_*`
    /// are set after `plan.env` in project sessions, and `TERM` is set last.
    Refused,
}

/// The overrides for the source's `servers`, by name. `launch_env` is what the launch already sets
/// on the Codex process beyond the shell's. `reserved` classifies a
/// variable name for a server's `env` (see [`Reserved`]). `taken` says whether Codex's own
/// configuration already defines a server of that name, which is then left alone; the reserved
/// `octoboard` name is never overridden either.
pub fn codex_servers(
    servers: BTreeMap<String, Value>,
    shell_env: &HashMap<String, String>,
    launch_env: &[(String, String)],
    reserved: &dyn Fn(&str) -> Reserved,
    taken: &dyn Fn(&str) -> bool,
) -> Servers {
    let mut pool = EnvPool::new(shell_env, launch_env, reserved);
    let mut out = Servers::default();
    for (index, (name, raw)) in servers.into_iter().enumerate() {
        if name == crate::session_tools::catalogue::SERVER_KEY
            || taken(&name)
            || !is_codex_name(&name)
        {
            continue;
        }
        let mut staged = pool.clone();
        match translate(&name, &raw, shell_env, index, &mut staged) {
            Ok(server) => {
                pool = staged;
                out.overrides.extend(server.overrides(&name));
            }
            Err(reason) => {
                tracing::warn!(server = %name, %reason, "skipping a Claude Code MCP server for Codex");
            }
        }
    }
    out.env = pool.added.into_iter().collect();
    out
}

/// Codex keys a server by a name of letters, digits, `-` and `_`.
fn is_codex_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// A server in Codex's terms.
#[derive(Debug, PartialEq)]
enum Server {
    Stdio {
        command: String,
        args: Vec<String>,
        cwd: Option<String>,
        env_vars: Vec<String>,
    },
    Http {
        url: String,
        /// Header name and the name of the variable holding its value.
        headers: Vec<(String, String)>,
    },
}

impl Server {
    fn overrides(&self, name: &str) -> Vec<String> {
        let key = format!("mcp_servers.{name}");
        let list = |items: &[String]| {
            format!(
                "[{}]",
                items
                    .iter()
                    .map(|item| toml_string(item))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        };
        let mut out = Vec::new();
        match self {
            Server::Stdio {
                command,
                args,
                cwd,
                env_vars,
            } => {
                out.push(format!("{key}.command={}", toml_string(command)));
                if !args.is_empty() {
                    out.push(format!("{key}.args={}", list(args)));
                }
                if let Some(cwd) = cwd {
                    out.push(format!("{key}.cwd={}", toml_string(cwd)));
                }
                if !env_vars.is_empty() {
                    out.push(format!("{key}.env_vars={}", list(env_vars)));
                }
            }
            Server::Http { url, headers } => {
                out.push(format!("{key}.url={}", toml_string(url)));
                if !headers.is_empty() {
                    let table = headers
                        .iter()
                        .map(|(header, var)| {
                            format!("{}={}", toml_string(header), toml_string(var))
                        })
                        .collect::<Vec<_>>()
                        .join(",");
                    out.push(format!("{key}.env_http_headers={{{table}}}"));
                }
            }
        }
        out
    }
}

/// The variables the Codex process will hold beyond the shell's, so that two servers never ask for
/// one name with two values.
#[derive(Clone)]
struct EnvPool<'a> {
    shell_env: &'a HashMap<String, String>,
    launch_env: &'a [(String, String)],
    reserved: &'a dyn Fn(&str) -> Reserved,
    added: BTreeMap<String, String>,
}

impl<'a> EnvPool<'a> {
    fn new(
        shell_env: &'a HashMap<String, String>,
        launch_env: &'a [(String, String)],
        reserved: &'a dyn Fn(&str) -> Reserved,
    ) -> Self {
        Self {
            shell_env,
            launch_env,
            reserved,
            added: BTreeMap::new(),
        }
    }

    /// The value the launch sets under `name` beyond the shell's.
    fn launched(&self, name: &str) -> Option<&str> {
        self.launch_env
            .iter()
            .rev()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// The value the Codex process holds under `name` before any server adds to it.
    fn held(&self, name: &str) -> Option<&str> {
        self.launched(name)
            .or_else(|| self.shell_env.get(name).map(String::as_str))
    }

    /// Makes the Codex process hold `value` under `name` for a server's `env` entry, if that is
    /// possible without changing what the launch, the shell or an earlier server set.
    fn provide(&mut self, name: &str, value: &str) -> bool {
        match (self.reserved)(name) {
            Reserved::Refused => false,
            Reserved::Held => self.held(name) == Some(value),
            Reserved::No => match self.held(name) {
                Some(have) => have == value,
                None => self.provide_own(name, value),
            },
        }
    }

    /// The same for a variable of Octoboard's own naming, which is not subject to `reserved`. The
    /// shell is not looked at: the launch's variables are applied over it, so an inherited value
    /// is replaced.
    fn provide_own(&mut self, name: &str, value: &str) -> bool {
        if let Some(have) = self.launched(name) {
            return have == value;
        }
        match self.added.get(name) {
            Some(have) => have == value,
            None => {
                self.added.insert(name.to_owned(), value.to_owned());
                true
            }
        }
    }
}

fn translate(
    name: &str,
    raw: &Value,
    shell_env: &HashMap<String, String>,
    index: usize,
    pool: &mut EnvPool<'_>,
) -> Result<Server, String> {
    let object = raw.as_object().ok_or("not an object")?;
    if object.contains_key("headersHelper") {
        return Err("uses headersHelper".into());
    }
    let text = |key: &str| -> Result<Option<String>, String> {
        match object.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(value)) => expand(value, shell_env).map(|(text, _)| Some(text)),
            Some(_) => Err(format!("`{key}` is not a string")),
        }
    };
    // The `url` goes on Codex's own argv: no value may come from a variable, which could hold a
    // secret, and none may carry credentials or a query, where a token usually sits.
    let url_text = |value: &str| -> Result<String, String> {
        match expand(value, shell_env)? {
            (_, true) => Err("`url` takes a value from a variable".into()),
            (text, false) if has_credentials_or_query(&text) => {
                Err("`url` carries credentials or a query string".into())
            }
            (text, false) => Ok(text),
        }
    };
    let strings = |value: Option<&Value>, what: &str| -> Result<Vec<(String, String)>, String> {
        let Some(value) = value else {
            return Ok(Vec::new());
        };
        let map = value
            .as_object()
            .ok_or(format!("`{what}` is not an object"))?;
        map.iter()
            .map(|(key, value)| {
                let value = value
                    .as_str()
                    .ok_or(format!("`{what}` holds a non-string"))?;
                let value = expand(value, shell_env)?.0;
                if value.contains('\0') {
                    return Err(format!("`{what}.{key}` holds a NUL byte"));
                }
                Ok((key.clone(), value))
            })
            .collect()
    };
    match object
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("stdio")
    {
        "stdio" => {
            let command = match object.get("command") {
                Some(Value::String(value)) => expand(value, shell_env)?.0,
                None | Some(Value::Null) => return Err("no `command`".into()),
                Some(_) => return Err("`command` is not a string".into()),
            };
            let args = match object.get("args") {
                None => Vec::new(),
                Some(Value::Array(items)) => items
                    .iter()
                    .map(|item| {
                        let item = item.as_str().ok_or("`args` holds a non-string")?;
                        Ok::<_, String>(expand(item, shell_env)?.0)
                    })
                    .collect::<Result<_, _>>()?,
                Some(_) => return Err("`args` is not an array".into()),
            };
            let mut env_vars = Vec::new();
            for (key, value) in strings(object.get("env"), "env")? {
                if !is_env_name(&key) {
                    return Err(format!("`env.{key}` is not a valid variable name"));
                }
                if !pool.provide(&key, &value) {
                    return Err(format!(
                        "`env.{key}` would need a value the Codex process cannot hold"
                    ));
                }
                env_vars.push(key);
            }
            Ok(Server::Stdio {
                command,
                args,
                cwd: text("cwd")?,
                env_vars,
            })
        }
        "http" => {
            let url = match object.get("url") {
                Some(Value::String(value)) => url_text(value)?,
                None | Some(Value::Null) => return Err("no `url`".into()),
                Some(_) => return Err("`url` is not a string".into()),
            };
            let mut headers = Vec::new();
            for (position, (header, value)) in strings(object.get("headers"), "headers")?
                .into_iter()
                .enumerate()
            {
                let var = format!("{HEADER_ENV_PREFIX}{index}_{position}{HEADER_ENV_SUFFIX}");
                if !pool.provide_own(&var, &value) {
                    return Err(format!(
                        "header `{header}` would need a value the Codex process cannot hold"
                    ));
                }
                headers.push((header, var));
            }
            Ok(Server::Http { url, headers })
        }
        other => Err(format!("transport `{other}` is not supported")),
    }
    .inspect(|_| tracing::debug!(server = %name, "passing a Claude Code MCP server to Codex"))
}

/// Whether the name can be an environment variable's: letters, digits and `_`, not starting with a
/// digit.
fn is_env_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Whether a URL has `user:pass@` in front of its host or a query string.
fn has_credentials_or_query(url: &str) -> bool {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    authority.contains('@') || url.split('#').next().is_some_and(|url| url.contains('?'))
}

/// Expands `${VAR}` and `${VAR:-default}` the way Claude Code does: a variable the environment
/// does not hold takes its default, and without one the server cannot be started. The flag says
/// whether a value of the environment went into the text.
fn expand(text: &str, env: &HashMap<String, String>) -> Result<(String, bool), String> {
    let mut out = String::new();
    let mut from_env = false;
    let mut rest = text;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else {
            out.push_str(&rest[start..]);
            return Ok((out, from_env));
        };
        let inner = &after[..end];
        let (var, default) = match inner.split_once(":-") {
            Some((var, default)) => (var, Some(default)),
            None => (inner, None),
        };
        match (env.get(var), default) {
            (Some(value), _) => {
                out.push_str(value);
                from_env = true;
            }
            (None, Some(default)) => out.push_str(default),
            (None, None) => return Err(format!("`{var}` is not set")),
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Ok((out, from_env))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The servers a source offers, by name, from the JSON object of them.
    fn servers(object: Value) -> BTreeMap<String, Value> {
        object.as_object().unwrap().clone().into_iter().collect()
    }

    fn none(_: &str) -> bool {
        false
    }

    fn reserved(name: &str) -> Reserved {
        match name {
            "TERM" => Reserved::Refused,
            "CODEX_HOME" => Reserved::Held,
            _ if name.starts_with("OPENAI_") => Reserved::Held,
            _ => Reserved::No,
        }
    }

    #[test]
    fn expansion_follows_claude_code() {
        let env = HashMap::from([
            ("A".to_string(), "1".to_string()),
            ("E".to_string(), String::new()),
        ]);
        assert_eq!(
            expand("x-${A}-${B:-d}-${E:-d}-${A:-d}", &env).unwrap(),
            ("x-1-d--1".to_string(), true)
        );
        assert_eq!(expand("${B:-d}", &env).unwrap(), ("d".to_string(), false));
        assert!(expand("${B}", &env).is_err());
        assert_eq!(
            expand("${open", &env).unwrap(),
            ("${open".to_string(), false)
        );
    }

    fn mapped(raw: Value, env: &HashMap<String, String>) -> Result<(Server, EnvPool<'_>), String> {
        let mut pool = EnvPool::new(env, &[], &reserved);
        translate("s", &raw, env, 0, &mut pool).map(|server| (server, pool))
    }

    #[test]
    fn stdio_is_translated_and_env_values_stay_off_argv() {
        let env = HashMap::from([("SHELL_TOKEN".to_string(), "sekret".to_string())]);
        let (server, pool) = mapped(
            json!({"command": "npx", "args": ["-y", "pkg"], "cwd": "/w",
                   "env": {"SHELL_TOKEN": "${SHELL_TOKEN}", "OTHER": "v-${SHELL_TOKEN}"}}),
            &env,
        )
        .unwrap();
        let overrides = server.overrides("s");
        assert_eq!(
            overrides,
            [
                "mcp_servers.s.command=\"npx\"",
                "mcp_servers.s.args=[\"-y\",\"pkg\"]",
                "mcp_servers.s.cwd=\"/w\"",
                "mcp_servers.s.env_vars=[\"SHELL_TOKEN\",\"OTHER\"]",
            ]
        );
        assert!(!overrides.iter().any(|o| o.contains("v-sekret")));
        assert_eq!(
            pool.added,
            BTreeMap::from([("OTHER".to_string(), "v-sekret".to_string())])
        );
    }

    #[test]
    fn an_env_entry_that_clashes_with_the_shell_or_another_server_skips_the_server() {
        let env = HashMap::from([("K".to_string(), "shell".to_string())]);
        assert!(mapped(json!({"command": "c", "env": {"K": "other"}}), &env).is_err());

        let servers = servers(json!({
            "a": {"command": "a", "env": {"X": "1"}},
            "b": {"command": "b", "env": {"X": "2"}}
        }));
        let out = codex_servers(servers, &HashMap::new(), &[], &reserved, &none);
        assert!(out
            .overrides
            .iter()
            .any(|o| o.starts_with("mcp_servers.a.")));
        assert!(!out
            .overrides
            .iter()
            .any(|o| o.starts_with("mcp_servers.b.")));
        assert_eq!(out.env, [("X".to_string(), "1".to_string())]);
    }

    #[test]
    fn http_headers_travel_in_the_environment() {
        let env = HashMap::from([("T".to_string(), "tok".to_string())]);
        let (server, pool) = mapped(
            json!({"type": "http", "url": "https://x.example/mcp", "headers": {"Authorization": "Bearer ${T}"}}),
            &env,
        )
        .unwrap();
        let overrides = server.overrides("s");
        assert_eq!(overrides[0], "mcp_servers.s.url=\"https://x.example/mcp\"");
        assert_eq!(
            overrides[1],
            "mcp_servers.s.env_http_headers={\"Authorization\"=\"OCTOBOARD_CLAUDE_MCP_0_0_TOKEN\"}"
        );
        assert!(!overrides.iter().any(|o| o.contains("tok")));
        assert_eq!(pool.added["OCTOBOARD_CLAUDE_MCP_0_0_TOKEN"], "Bearer tok");
    }

    #[test]
    fn what_codex_cannot_express_is_skipped() {
        let env = HashMap::new();
        assert!(mapped(json!({"type": "sse", "url": "https://x"}), &env).is_err());
        assert!(mapped(
            json!({"type": "http", "url": "https://x", "headersHelper": "h"}),
            &env
        )
        .is_err());
        assert!(mapped(json!({"command": "${MISSING}"}), &env).is_err());
        assert!(mapped(json!({"type": "http"}), &env).is_err());
    }

    #[test]
    fn variables_expand_in_the_command_args_and_cwd_but_not_the_url() {
        let env = HashMap::from([("H".to_string(), "/home/u".to_string())]);
        let (server, _) = mapped(
            json!({"command": "${H}/bin/s", "args": ["${H}/mcp/server.js", "${U:-plain}"], "cwd": "${H}"}),
            &env,
        )
        .unwrap();
        assert_eq!(
            server.overrides("s"),
            [
                "mcp_servers.s.command=\"/home/u/bin/s\"",
                "mcp_servers.s.args=[\"/home/u/mcp/server.js\",\"plain\"]",
                "mcp_servers.s.cwd=\"/home/u\"",
            ]
        );
        assert!(mapped(json!({"type": "http", "url": "https://x/${H}"}), &env).is_err());
        assert!(mapped(json!({"type": "http", "url": "https://x/${U:-p}"}), &env).is_ok());
    }

    #[test]
    fn a_url_with_credentials_or_a_query_is_skipped() {
        let env = HashMap::new();
        for url in [
            "https://user:pass@x.example/mcp",
            "https://tok@x.example",
            "https://x.example/mcp?key=1",
            "https://x.example?key=1",
        ] {
            assert!(
                mapped(json!({"type": "http", "url": url}), &env).is_err(),
                "{url}"
            );
        }
        for url in [
            "https://x.example/mcp",
            "https://x.example/a@b",
            "https://x.example/#a?b",
        ] {
            assert!(
                mapped(json!({"type": "http", "url": url}), &env).is_ok(),
                "{url}"
            );
        }
    }

    #[test]
    fn a_server_cannot_set_what_the_launch_or_codex_owns() {
        let env = HashMap::from([("OPENAI_BASE_URL".to_string(), "same".to_string())]);
        let launch = [("CODEX_HOME".to_string(), "/pinned".to_string())];
        let try_env = |key: &str, value: &str| {
            let mut pool = EnvPool::new(&env, &launch, &reserved);
            translate(
                "s",
                &json!({"command": "c", "env": {key: value}}),
                &env,
                0,
                &mut pool,
            )
            .is_ok()
        };
        assert!(!try_env("CODEX_HOME", "/elsewhere"));
        assert!(try_env("CODEX_HOME", "/pinned"));
        assert!(!try_env("TERM", "dumb"));
        // A name the launch sets afterwards is refused whatever the value.
        let term_shell = HashMap::from([("TERM".to_string(), "dumb".to_string())]);
        let mut pool = EnvPool::new(&term_shell, &[], &reserved);
        assert!(!pool.provide("TERM", "dumb"));
        assert!(!try_env("OPENAI_API_KEY", "k"));
        assert!(!try_env("OPENAI_BASE_URL", "other"));
        assert!(try_env("OPENAI_BASE_URL", "same"));
        assert!(try_env("MINE", "v"));
    }

    #[test]
    fn a_header_variable_inherited_from_the_shell_does_not_block_a_header_server() {
        let name = format!("{HEADER_ENV_PREFIX}0_0{HEADER_ENV_SUFFIX}");
        let env = HashMap::from([(name.clone(), "parent".to_string())]);
        let (server, pool) = mapped(
            json!({"type": "http", "url": "https://h", "headers": {"Authorization": "own"}}),
            &env,
        )
        .unwrap();
        assert!(matches!(server, Server::Http { .. }));
        assert_eq!(pool.added, BTreeMap::from([(name, "own".to_string())]));
    }

    #[test]
    fn a_bad_env_name_or_a_nul_skips_the_server() {
        let env = HashMap::new();
        for (key, value) in [
            ("A-B", "v"),
            ("1A", "v"),
            ("", "v"),
            ("A\0B", "v"),
            ("A", "v\0"),
        ] {
            assert!(
                mapped(json!({"command": "c", "env": {key: value}}), &env).is_err(),
                "{key:?}"
            );
        }
        assert!(mapped(
            json!({"type": "http", "url": "https://x", "headers": {"H": "a\0b"}}),
            &env
        )
        .is_err());
    }

    #[test]
    fn names_codex_owns_or_cannot_key_are_left_out() {
        let servers = servers(json!({
            "octoboard": {"command": "x"}, "mine": {"command": "x"},
            "has space": {"command": "x"}, "ok": {"command": "x"}
        }));
        let out = codex_servers(servers, &HashMap::new(), &[], &reserved, &|name| {
            name == "mine"
        });
        assert_eq!(out.overrides, ["mcp_servers.ok.command=\"x\""]);
    }
}
