//! Rewriting the paths a JSON file of another program's records, and nothing else in it: an
//! agent's plugin registry names the directory a plugin was installed under, and that name has to
//! follow when the directory is reached by another path. The file is the other program's, so
//! every byte this does not have to change is kept: only the string literals that hold a moved
//! path are replaced, and a file with nothing to move is not written at all.

use std::io;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::replace::{remove_stale_staged, replace_beside, Permissions};

/// Which strings of the document are paths to look at.
#[derive(Debug, Clone, Copy)]
pub enum Select {
    /// Every string, at any depth.
    Any,
    /// The strings at this place: each step is an object's key, or `*` for every value of an
    /// object or an array.
    At(&'static [&'static str]),
    /// Several selected paths that may hold strings.
    AnyOf(&'static [&'static [&'static str]]),
}

/// `rest` below `dir`, for a `moved` answer: `dir` itself, as it is spelled, when `rest` is
/// empty. `Path::join` would end that one with a separator, which the program that recorded the
/// directory did not write.
pub fn below(dir: &Path, rest: &Path) -> PathBuf {
    match rest.as_os_str().is_empty() {
        true => dir.to_path_buf(),
        false => dir.join(rest),
    }
}

/// How often a rewrite starts over because the file changed between its read and its replace.
const ATTEMPTS: usize = 3;

/// Serializes this daemon's replacements of such files, whoever makes them, so that two of them
/// do not replace one file with what each read before the other wrote. Held over a second read
/// and the replace only: the file is read and parsed outside it. The program that owns the file
/// takes no part in it.
fn replace_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Rewrites, in the JSON file `file`, each selected string that `moved` answers another path for
/// to that path, and answers whether it wrote. A file that is not there, or holds no such string,
/// is not written. A path that is not Unicode, either way, is left as it is: JSON could only hold
/// a damaged spelling of it.
///
/// A file that is not JSON cannot be looked into: it is an error where `matters_unread` says its
/// text matters to the caller all the same, and is left alone otherwise. So is one that is no
/// regular file or is larger than `limit`.
///
/// The new text is the old with the string literals of the moved paths replaced, so the file's
/// own layout, escapes and ending are kept; where that does not give exactly the document with
/// those strings changed (the same literal elsewhere, a path spelled with an escape of its own),
/// the document is written out afresh, keeping the file's ending. It is written through a file
/// beside the target and renamed over it (where it leads, for a link), so a reader never sees
/// half of it. The file is replaced only if it still holds the bytes that were read; if it keeps
/// changing, the rewrite gives up with an error. Before anything else, what an earlier rewrite
/// that its process's end cut short left staged beside the file is removed
/// ([`remove_stale_staged`]). Blocking.
pub fn rewrite_paths(
    file: &Path,
    limit: usize,
    select: Select,
    moved: &dyn Fn(&Path) -> Option<PathBuf>,
    matters_unread: &dyn Fn(&str) -> bool,
) -> io::Result<bool> {
    try_rewrite_paths(file, limit, select, &|path| Ok(moved(path)), matters_unread)
}

/// Like [`rewrite_paths`], with a path resolver that may refuse a selected path. On refusal the
/// entire registry keeps its original bytes.
pub fn try_rewrite_paths(
    file: &Path,
    limit: usize,
    select: Select,
    moved: &dyn Fn(&Path) -> io::Result<Option<PathBuf>>,
    matters_unread: &dyn Fn(&str) -> bool,
) -> io::Result<bool> {
    // What a rewrite of this file cut short by its process's end left staged beside it is cleared
    // away first, whether or not anything is rewritten now.
    remove_stale_staged(&super::canonical(file).unwrap_or_else(|_| file.to_path_buf()));
    for _ in 0..ATTEMPTS {
        let text = match super::read_regular_text(file, limit) {
            Ok(text) => text,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(err) => return Err(err),
        };
        let Some(rewritten) = try_rewritten(&text, select, moved, matters_unread)
            .map_err(|err| io::Error::other(format!("`{}` {err}", file.display())))?
        else {
            return Ok(false);
        };
        let target = super::canonical(file)?;
        let _guard = replace_lock();
        // Read again under the lock: another rewrite of this daemon's, or the file's own program,
        // may have written since, and replacing would lose that.
        match std::fs::read(&target) {
            Ok(now) if now == text.as_bytes() => {
                replace_beside(&target, rewritten.as_bytes(), Permissions::KeptOrDefault)?;
                return Ok(true);
            }
            Ok(_) => continue,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(err) => return Err(err),
        }
    }
    Err(io::Error::other(format!(
        "`{}` kept changing while its paths were being rewritten",
        file.display()
    )))
}

/// The document `text` with the moved paths replaced; `None` when there is nothing to move, or
/// the text is not JSON and does not matter. The error says what is wrong with the file.
#[cfg(test)]
fn rewritten(
    text: &str,
    select: Select,
    moved: &dyn Fn(&Path) -> Option<PathBuf>,
    matters_unread: &dyn Fn(&str) -> bool,
) -> Result<Option<String>, String> {
    try_rewritten(text, select, &|path| Ok(moved(path)), matters_unread)
}

fn try_rewritten(
    text: &str,
    select: Select,
    moved: &dyn Fn(&Path) -> io::Result<Option<PathBuf>>,
    matters_unread: &dyn Fn(&str) -> bool,
) -> Result<Option<String>, String> {
    let mut after: Value = match serde_json::from_str(text) {
        Ok(value) => value,
        Err(_) if !matters_unread(text) => return Ok(None),
        Err(err) => return Err(format!("is not JSON: {err}")),
    };
    let mut changes: Vec<(String, String)> = Vec::new();
    let mut refusal = None;
    let mut change = |string: &mut String| {
        let new = match moved(Path::new(string.as_str())) {
            Ok(Some(new)) => new,
            Ok(None) => return,
            Err(err) => {
                refusal.get_or_insert(err);
                return;
            }
        };
        let Some(new) = new.to_str() else {
            return;
        };
        if new != string {
            changes.push((string.clone(), new.to_string()));
            *string = new.to_string();
        }
    };
    match select {
        Select::Any => each_string(&mut after, &mut change),
        Select::At(steps) => each_string_at(&mut after, steps, &mut change),
        Select::AnyOf(paths) => {
            for steps in paths {
                each_string_at(&mut after, steps, &mut change);
            }
        }
    }
    if let Some(err) = refusal {
        return Err(err.to_string());
    }
    if changes.is_empty() {
        return Ok(None);
    }
    let mut replaced = text.to_string();
    for (old, new) in &changes {
        let literal =
            |string: &String| serde_json::to_string(string).map_err(|err| err.to_string());
        replaced = replaced.replace(&literal(old)?, &literal(new)?);
    }
    if serde_json::from_str::<Value>(&replaced).ok().as_ref() == Some(&after) {
        return Ok(Some(replaced));
    }
    let mut afresh = serde_json::to_string_pretty(&after).map_err(|err| err.to_string())?;
    afresh.push_str(&text[text.trim_end().len()..]);
    Ok(Some(afresh))
}

fn each_string(value: &mut Value, change: &mut dyn FnMut(&mut String)) {
    match value {
        Value::String(string) => change(string),
        Value::Array(items) => items.iter_mut().for_each(|item| each_string(item, change)),
        Value::Object(map) => map.values_mut().for_each(|item| each_string(item, change)),
        _ => {}
    }
}

fn each_string_at(value: &mut Value, steps: &[&str], change: &mut dyn FnMut(&mut String)) {
    let Some((step, rest)) = steps.split_first() else {
        if let Value::String(string) = value {
            change(string);
        }
        return;
    };
    match value {
        Value::Object(map) if *step == "*" => map
            .values_mut()
            .for_each(|item| each_string_at(item, rest, change)),
        Value::Array(items) if *step == "*" => items
            .iter_mut()
            .for_each(|item| each_string_at(item, rest, change)),
        Value::Object(map) => {
            if let Some(item) = map.get_mut(*step) {
                each_string_at(item, rest, change);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ScratchDir;

    const LIMIT: usize = 1024 * 1024;

    fn under(from: &'static str, to: &'static str) -> impl Fn(&Path) -> Option<PathBuf> {
        move |path| {
            path.strip_prefix(from)
                .ok()
                .map(|rest| below(Path::new(to), rest))
        }
    }

    fn rewritten_any(text: &str, moved: &dyn Fn(&Path) -> Option<PathBuf>) -> Option<String> {
        rewritten(text, Select::Any, moved, &|_| true).unwrap()
    }

    /// Only the literals of the moved paths change: the layout, the other strings and the ending
    /// of the file stay byte for byte.
    #[test]
    fn only_the_moved_paths_literals_change() {
        let text = "{\"a\":   [ \"/old/x\",\"/kept/y\" ],\n\t\"b\": {\"p\": \"/old\"}}\r\n\r\n";
        let moved = under("/old", "/new");
        assert_eq!(
            rewritten_any(text, &moved).unwrap(),
            "{\"a\":   [ \"/new/x\",\"/kept/y\" ],\n\t\"b\": {\"p\": \"/new\"}}\r\n\r\n"
        );
        assert_eq!(rewritten_any("{\"a\": \"/kept\"}  ", &moved), None);
    }

    /// A selector looks only at its place: the same path elsewhere is not a path to move.
    #[test]
    fn a_selector_reaches_only_its_place() {
        let text = r#"{"repos":{"r":{"path":"/old/x","source":"/old/x"},"s":{"path":"/kept"}},"path":"/old/y"}"#;
        let moved = under("/old", "/new");
        let select = Select::At(&["repos", "*", "path"]);
        let out = rewritten(text, select, &moved, &|_| true).unwrap().unwrap();
        let value: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(value["repos"]["r"]["path"], "/new/x");
        // The same literal outside the selected place is not changed, so the text could not be
        // had by replacing literals and was written out afresh.
        assert_eq!(value["repos"]["r"]["source"], "/old/x");
        assert_eq!(value["path"], "/old/y");
        assert_eq!(value["repos"]["s"]["path"], "/kept");

        let arrays = r#"{"list":[{"path":"/old/a"},{"path":"/kept"}]}"#;
        let out = rewritten(arrays, Select::At(&["list", "*", "path"]), &moved, &|_| {
            true
        })
        .unwrap()
        .unwrap();
        assert_eq!(out, r#"{"list":[{"path":"/new/a"},{"path":"/kept"}]}"#);
    }

    /// A path the file spells with escapes of its own (a writer that escapes `/`, or writes a
    /// character by its code) is found by what it says, and the file's ending is kept when it
    /// is written out afresh.
    #[test]
    fn a_path_spelled_with_other_escapes_is_found_by_what_it_says() {
        let moved = under("/old dir", "/new");
        for text in [
            "{\"p\":\"\\/old dir\\/x\"}\n\n",
            "{\"p\":\"/old\\u0020dir/x\"}\n\n",
        ] {
            let out = rewritten_any(text, &moved).expect("rewritten");
            let value: Value = serde_json::from_str(&out).unwrap();
            assert_eq!(value["p"], "/new/x", "{text}");
            assert!(
                out.ends_with("}\n\n") && !out.ends_with("}\n\n\n"),
                "{out:?}"
            );
        }
    }

    /// On Windows a path recorded with one separator is the path spelled with the other.
    #[cfg(windows)]
    #[test]
    fn a_path_spelled_with_the_other_separator_is_found() {
        let moved = |path: &Path| {
            path.strip_prefix("C:/Users/u/.agent-work/plugins")
                .ok()
                .map(|rest| Path::new("C:/Users/u/.agent/plugins").join(rest))
        };
        let text = r#"{"p":"C:\\Users\\u\\.agent-work\\plugins\\cache\\tool"}"#;
        let out = rewritten_any(text, &moved).expect("rewritten");
        let value: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(
            Path::new(value["p"].as_str().unwrap()),
            Path::new("C:/Users/u/.agent/plugins/cache/tool")
        );
    }

    /// A path that is not Unicode cannot be written into JSON whole, so the string stays.
    #[cfg(unix)]
    #[test]
    fn a_path_that_is_not_unicode_is_left_as_it_is() {
        use std::os::unix::ffi::OsStringExt;
        let moved = |_: &Path| {
            Some(PathBuf::from(std::ffi::OsString::from_vec(vec![
                b'/', 0xff, b'x',
            ])))
        };
        assert_eq!(rewritten_any(r#"{"p":"/old/x"}"#, &moved), None);
    }

    /// A file that is not JSON is an error only where the caller says its text matters.
    #[test]
    fn a_file_that_is_not_json_is_an_error_only_where_it_matters() {
        let moved = under("/old", "/new");
        assert_eq!(
            rewritten("not json /old", Select::Any, &moved, &|_| false),
            Ok(None)
        );
        let err = rewritten("not json /old", Select::Any, &moved, &|text| {
            text.contains("/old")
        })
        .unwrap_err();
        assert!(err.contains("is not JSON"), "{err}");
    }

    /// The file is written only when something moved, where it leads when it is a link, and a
    /// missing one is not created.
    #[test]
    fn a_file_is_written_only_when_something_moved() {
        let dir = ScratchDir::new("json-paths-file");
        let real = dir.join("real.json");
        let link = dir.join("link.json");
        let moved = under("/old", "/new");
        assert!(!rewrite_paths(&real, LIMIT, Select::Any, &moved, &|_| true).unwrap());
        assert!(!real.exists());

        std::fs::write(&real, r#"{"p": "/kept"}"#).unwrap();
        crate::test_support::symlink(&real, &link).unwrap();
        let identity = |path: &Path| {
            crate::platform::fs::identity(path, crate::platform::fs::Links::Keep).unwrap()
        };
        let before = identity(&real);
        assert!(!rewrite_paths(&link, LIMIT, Select::Any, &moved, &|_| true).unwrap());
        assert!(identity(&real) == before, "an unchanged file was replaced");

        std::fs::write(&real, r#"{"p": "/old/x"}"#).unwrap();
        assert!(rewrite_paths(&link, LIMIT, Select::Any, &moved, &|_| true).unwrap());
        assert_eq!(
            std::fs::read_to_string(&real).unwrap(),
            r#"{"p": "/new/x"}"#
        );
        assert!(crate::platform::fs::read_alias(&link).unwrap().is_some());
        // Nothing staged is left beside it.
        assert_eq!(dir.read_dir().unwrap().count(), 2);
    }
}
