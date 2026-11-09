//! What the providers that consume a compatibility source share in offering its configuration:
//! its MCP servers in the consumer's terms, with the values that may be secrets kept out of
//! anything that is listed, and its skills and prompts linked into a directory of the session's.
//!
//! In the common MCP format a value that holds `${` is a reference to an environment variable the
//! consumer's process holds (`CompatibilitySource::mcp_servers`). A consumer expands such a
//! reference in the `args`, `env` values and `headers` it is given. A `command`, `args` entry or
//! `url` holding `${` is refused here, since the consumer cannot tell a literal from a reference
//! there and the rule stays the same for every consumer.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

/// A server name of letters, digits, `-` and `_`, as the consumers key their servers.
pub(super) fn is_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Whether `name` can name an environment variable: letters, digits and `_`, not starting with a
/// digit.
pub(super) fn is_env_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A server of the source's in the consumer's terms (`type`, `command`, `args` and `env`, or
/// `type`, `url` and `headers`), the variables it needs pushed on `env`; `None` for one that cannot
/// be given. A literal `env` or header value is replaced by a reference to a variable named
/// `<prefix><index>_<position>` that carries it, so that no value that may be a secret is written
/// where it can be listed; `index` is the server's position among the source's, `position` the
/// entry's within its `env` or `headers`. A value that holds a reference already stays as it is,
/// unless `unset_in` is given, the environment the source's agent would have forwarded from: then
/// an entry that is just a `${NAME}` of a variable it lacks is dropped, and a server whose other
/// value refers to one is left out, as the source's agent would have skipped it.
pub(super) fn translate(
    raw: &Value,
    prefix: &str,
    index: usize,
    unset_in: Option<&HashMap<String, String>>,
    env: &mut Vec<(String, String)>,
) -> Option<Value> {
    let object = raw.as_object()?;
    let on_argv =
        |text: &str| (!text.contains("${") && !text.contains('\0')).then(|| text.to_owned());
    match object
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("stdio")
    {
        "stdio" => {
            let command = on_argv(object.get("command")?.as_str()?)?;
            let args = match object.get("args") {
                None => Vec::new(),
                Some(args) => args
                    .as_array()?
                    .iter()
                    .map(|arg| on_argv(arg.as_str()?))
                    .collect::<Option<_>>()?,
            };
            let mut server = json!({"type": "stdio", "command": command, "args": args});
            let values = values(object.get("env"), prefix, index, unset_in, env, is_env_name)?;
            if !values.is_empty() {
                server["env"] = Value::Object(values);
            }
            Some(server)
        }
        kind @ ("http" | "sse") => {
            let url = on_argv(object.get("url")?.as_str()?)?;
            let mut server = json!({"type": kind, "url": url});
            let values = values(
                object.get("headers"),
                prefix,
                index,
                unset_in,
                env,
                |name| !name.is_empty(),
            )?;
            if !values.is_empty() {
                server["headers"] = Value::Object(values);
            }
            Some(server)
        }
        _ => None,
    }
}

/// The entries of an `env` or `headers` object, a literal value replaced by a reference to a
/// variable that carries it (pushed on `env`), a value that holds a reference already kept (or
/// settled against `unset_in`, see [`translate`]); `None` for a non-object, a non-string, a name
/// `valid` refuses or a value with a NUL byte. Nothing is pushed on `env` for a `None`.
fn values(
    value: Option<&Value>,
    prefix: &str,
    index: usize,
    unset_in: Option<&HashMap<String, String>>,
    env: &mut Vec<(String, String)>,
    valid: impl Fn(&str) -> bool,
) -> Option<Map<String, Value>> {
    let Some(value) = value else {
        return Some(Map::new());
    };
    let mut carried = Vec::new();
    let mut out = Map::new();
    for (position, (name, value)) in value.as_object()?.iter().enumerate() {
        let value = value.as_str().filter(|value| !value.contains('\0'))?;
        if !valid(name) {
            return None;
        }
        if value.contains("${") {
            match unset_in.map_or(Unset::None, |known| unset(value, known)) {
                Unset::None => {
                    out.insert(name.clone(), json!(value));
                }
                Unset::Whole => {}
                Unset::Part => return None,
            }
        } else {
            let var = format!("{prefix}{index}_{position}");
            out.insert(name.clone(), json!(format!("${{{var}}}")));
            carried.push((var, value.to_owned()));
        }
    }
    env.extend(carried);
    Some(out)
}

/// What a value's plain `${NAME}` references lack in the environment `known`.
enum Unset {
    /// Every one is set (or there is none).
    None,
    /// The value is one reference alone, to a variable that is not set.
    Whole,
    /// The value mixes text and a reference to a variable that is not set.
    Part,
}

/// Which of `value`'s `${NAME}` references name a variable `known` lacks. A `${NAME:-default}` is
/// never short of its variable.
fn unset(value: &str, known: &HashMap<String, String>) -> Unset {
    let mut missing = None;
    let mut rest = value;
    while let Some(at) = rest.find("${") {
        let after = &rest[at + 2..];
        let Some(end) = after.find('}') else {
            break;
        };
        let name = &after[..end];
        if is_env_name(name) && !known.contains_key(name) {
            missing = Some(name);
        }
        rest = &after[end + 1..];
    }
    match missing {
        None => Unset::None,
        Some(name) if value == format!("${{{name}}}") => Unset::Whole,
        Some(_) => Unset::Part,
    }
}

/// Makes `dir/<name>` for each of `entries`, `name` being the entry's own file name: a link to the
/// entry's resolved target, or, for a file where the host makes no file links, a copy of it. A
/// directory is linked on every host, since a directory alias needs none. Returns how many were
/// made; one that cannot be made is left out, with a log naming the `consumer`.
pub(super) fn offer_entries(
    consumer: &str,
    dir: &Path,
    entries: Vec<PathBuf>,
    is_dir: bool,
    file_links: bool,
) -> usize {
    let mut made = 0;
    for from in entries {
        let Some(name) = from.file_name() else {
            continue;
        };
        let target = crate::hostfs::canonical(&from).unwrap_or(from.clone());
        let at = dir.join(name);
        let result = std::fs::create_dir_all(dir).and_then(|()| {
            if is_dir || file_links {
                crate::hostfs::alias(&target, &at, is_dir)
            } else {
                std::fs::copy(&target, &at).map(drop)
            }
        });
        match result {
            Ok(()) => made += 1,
            Err(err) => {
                tracing::warn!(path = %from.display(), %err, "could not offer a compatibility source's entry to {consumer}");
            }
        }
    }
    made
}
