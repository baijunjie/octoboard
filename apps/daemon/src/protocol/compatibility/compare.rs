//! The comparison of one file with its snapshot, in the direction of the file.

use std::collections::BTreeSet;
use std::fmt;

use serde_json::{Map, Value};

use super::norm::{normalize, Norm, Obj};
use super::WAY_OUT;
use crate::protocol::schema::Direction;

/// The failures of one `current` file against its `snapshot`.
pub(super) fn compare(
    file: &str,
    direction: Direction,
    snapshot: &Value,
    current: &Value,
) -> Vec<String> {
    let definitions = |schema: &'_ Value| schema.get("$defs").and_then(Value::as_object).cloned();
    let mut comparison = Comparison {
        file,
        direction,
        old_defs: definitions(snapshot),
        new_defs: definitions(current),
        failures: Vec::new(),
        count: 0,
        passed: BTreeSet::new(),
    };
    comparison.compare("#", snapshot, current);
    comparison.failures
}

struct Comparison<'a> {
    file: &'a str,
    direction: Direction,
    old_defs: Option<Map<String, Value>>,
    new_defs: Option<Map<String, Value>>,
    failures: Vec<String>,
    /// Every failure raised, counted even when its message was already given, to tell whether one
    /// comparison failed.
    count: usize,
    /// The pairs of definitions found to hold, or being compared, which a recursive type would
    /// otherwise compare for ever.
    passed: BTreeSet<(String, String)>,
}

impl Comparison<'_> {
    fn fail(&mut self, path: &str, rule: impl fmt::Display) {
        self.count += 1;
        let message = format!("{} {path}: {rule}; {WAY_OUT}", self.file);
        if !self.failures.contains(&message) {
            self.failures.push(message);
        }
    }

    /// Whether `new` keeps what a client built against `old` relies on, in this direction.
    fn compare(&mut self, path: &str, old: &Value, new: &Value) -> bool {
        let before = self.count;
        // A node that reaches a definition is keyed by itself on both sides, so that a type that
        // contains itself, however the reference is spelled (`anyOf` with `null`, with siblings),
        // is compared once.
        let key = (follows_reference(old) || follows_reference(new))
            .then(|| (old.to_string(), new.to_string()));
        if let Some(key) = &key {
            if !self.passed.insert(key.clone()) {
                return true;
            }
        }
        // Past a reference the path is the definition's, which is where it is in the file.
        let path = reference(new).map_or(path.to_string(), |name| format!("#/$defs/{name}"));
        match (
            normalize(self.old_defs.as_ref(), old),
            normalize(self.new_defs.as_ref(), new),
        ) {
            (Ok(old), Ok(new)) => match self.direction {
                Direction::Input => self.widen(&path, &old, &new),
                Direction::Output => self.give_more(&path, &old, &new),
            },
            (Err(problem), _) | (_, Err(problem)) => self.fail(&path, problem),
        }
        let held = self.count == before;
        if let (false, Some(key)) = (held, key) {
            self.passed.remove(&key);
        }
        held
    }

    /// An input: `new` has to accept everything `old` accepted.
    fn widen(&mut self, path: &str, old: &Norm, new: &Norm) {
        if new.any {
            return;
        }
        if old.any {
            self.fail(path, "accepted any value and now accepts only some");
            return;
        }
        for (kind, was, is) in [
            ("null", old.null, new.null),
            ("a boolean", old.boolean, new.boolean),
            ("a number", old.number, new.number),
        ] {
            if was && !is {
                self.fail(path, format!("no longer accepts {kind}"));
            }
        }
        if let Some(was) = &old.integer {
            match &new.integer {
                Some(is) if !is.contains(was) => self.fail(
                    path,
                    format!("accepted integers {was} and now accepts only {is}"),
                ),
                None if !new.number => self.fail(path, "no longer accepts an integer"),
                _ => {}
            }
        }
        if let Some(was) = &old.string {
            match (was, &new.string) {
                (_, None) => self.fail(path, "no longer accepts a string"),
                (_, Some(None)) => {}
                (None, Some(Some(is))) => self.fail(
                    path,
                    format!("accepted any string and now accepts only {is:?}"),
                ),
                (Some(was), Some(Some(is))) => {
                    let lost: Vec<_> = was.difference(is).collect();
                    if !lost.is_empty() {
                        self.fail(path, format!("no longer accepts the values {lost:?}"));
                    }
                }
            }
        }
        if let Some(was) = &old.array {
            match (was, &new.array) {
                (_, None) => self.fail(path, "no longer accepts an array"),
                (Some(was), Some(Some(is))) => {
                    self.compare(&format!("{path}/items"), was, is);
                }
                (None, Some(Some(_))) => {
                    self.fail(
                        path,
                        "accepted arrays of anything and now limits their items",
                    );
                }
                _ => {}
            }
        }
        self.match_objects(path, old, new);
    }

    /// An output: `new` has to give what `old` promised, and may give more.
    fn give_more(&mut self, path: &str, old: &Norm, new: &Norm) {
        if old.any {
            return;
        }
        if new.any {
            self.fail(path, "gives a value of any kind where it gave only some");
            return;
        }
        for (kind, was, is) in [
            ("null", old.null, new.null),
            ("a boolean", old.boolean, new.boolean),
            ("a number", old.number, new.number),
            ("an integer", old.integer.is_some(), new.integer.is_some()),
            ("a string", old.string.is_some(), new.string.is_some()),
            ("an array", old.array.is_some(), new.array.is_some()),
            (
                "an object",
                !old.objects.is_empty(),
                !new.objects.is_empty(),
            ),
        ] {
            if is && !was {
                self.fail(path, format!("gains a new kind of value, {kind}"));
            }
            if was && !is {
                self.fail(path, format!("loses a type it gave, {kind}"));
            }
        }
        if let (Some(was), Some(is)) = (&old.integer, &new.integer) {
            if !was.contains(is) {
                self.fail(path, format!("gave integers {was} and now gives {is}"));
            }
        }
        if let (Some(Some(was)), Some(is)) = (&old.array, &new.array) {
            match is {
                Some(is) => {
                    self.compare(&format!("{path}/items"), was, is);
                }
                None => self.fail(path, "gave arrays of one kind of item and now of anything"),
            }
        }
        self.match_objects(path, old, new);
    }

    /// How the objects of the two schemas pair up, by direction. An old alternative is removed
    /// when no new one carries all of its tag; a new alternative may be tagged further than the
    /// old one, as a variant that was split.
    /// - An input has to accept each old alternative: some new alternative that holds is enough.
    /// - An output may be any new alternative a client takes for an old variant, so each new
    ///   alternative that carries the whole tag of one or more old ones has to keep at least one
    ///   of them. One that carries no old variant's whole tag is a new variant and is not looked
    ///   at; an untagged old alternative has an empty tag, so every new one is taken for it.
    fn match_objects(&mut self, path: &str, old: &Norm, new: &Norm) {
        let carries = |candidate: &Obj, tag: &[(String, String)]| {
            let carried = candidate.tag();
            tag.iter().all(|entry| carried.contains(entry))
        };
        let mut removed = Vec::new();
        for was in &old.objects {
            let tag = was.tag();
            if !new.objects.iter().any(|candidate| carries(candidate, &tag)) {
                removed.push(was);
            }
        }
        for was in &removed {
            let tag = was.tag();
            let rule = if tag.is_empty() {
                "no alternative is left to take its place"
            } else {
                "the variant is removed"
            };
            self.fail(&tagged_path(path, &tag), rule);
        }
        match self.direction {
            Direction::Input => {
                for was in old
                    .objects
                    .iter()
                    .filter(|was| !removed.iter().any(|gone| std::ptr::eq(*gone, *was)))
                {
                    let tag = was.tag();
                    let pairs: Vec<(&Obj, &Obj)> = new
                        .objects
                        .iter()
                        .filter(|candidate| carries(candidate, &tag))
                        .map(|candidate| (was, candidate))
                        .collect();
                    self.hold_one_of(&tagged_path(path, &tag), pairs);
                }
            }
            Direction::Output => {
                for is in &new.objects {
                    let pairs: Vec<(&Obj, &Obj)> = old
                        .objects
                        .iter()
                        .filter(|was| carries(is, &was.tag()))
                        .map(|was| (was, is))
                        .collect();
                    if let Some((was, _)) = pairs.first() {
                        self.hold_one_of(&tagged_path(path, &was.tag()), pairs);
                    }
                }
            }
        }
    }

    /// Whether one of the (old, new) pairs holds. The first pair that does is the match; when
    /// none does, the first one's failures are the report. A trial that fails leaves nothing
    /// behind: what it counted is not a failure of the match, and the pairs it passed may have
    /// rested on a pair that failed.
    fn hold_one_of(&mut self, path: &str, pairs: Vec<(&Obj, &Obj)>) {
        let (count, passed) = (self.count, self.passed.clone());
        let mut report: Option<Vec<String>> = None;
        for (was, is) in pairs {
            let start = self.failures.len();
            let held = self.object(path, was, is);
            let raised = self.failures.split_off(start);
            self.count = count;
            if held {
                return;
            }
            self.passed = passed.clone();
            report.get_or_insert(raised);
        }
        if let Some(raised) = report {
            self.count += 1;
            for message in raised {
                if !self.failures.contains(&message) {
                    self.failures.push(message);
                }
            }
        }
    }

    fn object(&mut self, path: &str, old: &Obj, new: &Obj) -> bool {
        let before = self.count;
        let output = self.direction == Direction::Output;
        for (name, was) in &old.props {
            let at = format!("{path}/properties/{name}");
            match new.props.get(name) {
                None => self.fail(&at, "the property is removed"),
                Some(is) => {
                    self.compare(&at, was, is);
                    if output && old.required.contains(name) && !new.required.contains(name) {
                        self.fail(&at, "a required property may now be left out");
                    }
                }
            }
        }
        if !output {
            for name in new.required.difference(&old.required) {
                self.fail(
                    &format!("{path}/properties/{name}"),
                    "the property is newly required",
                );
            }
        }
        let map = format!("{path}/additionalProperties");
        match (&old.additional, &new.additional) {
            (Some(was), Some(is)) => {
                self.compare(&map, was, is);
            }
            (None, Some(_)) if !output => {
                self.fail(&map, "the values of a map are newly limited");
            }
            (Some(_), None) if output => {
                self.fail(&map, "the values of a map are no longer limited");
            }
            _ => {}
        }
        // The keys of an output map are an enumeration, which is not compared.
        if let (false, Some(was), Some(is)) = (output, &old.names, &new.names) {
            self.compare(&format!("{path}/propertyNames"), was, is);
        } else if let (false, None, Some(_)) = (output, &old.names, &new.names) {
            self.fail(
                &format!("{path}/propertyNames"),
                "the keys of a map are newly limited",
            );
        }
        self.count == before
    }
}

/// Whether normalizing the node follows a `$ref`: its own, or one of an `allOf`, `oneOf` or
/// `anyOf` member.
fn follows_reference(node: &Value) -> bool {
    let Some(object) = node.as_object() else {
        return false;
    };
    object.contains_key("$ref")
        || ["allOf", "oneOf", "anyOf"].iter().any(|keyword| {
            object
                .get(*keyword)
                .and_then(Value::as_array)
                .is_some_and(|members| members.iter().any(follows_reference))
        })
}

/// The name a node refers to when it is a bare `$ref` (a description, or a document's `$defs`, may
/// stand beside it).
fn reference(node: &Value) -> Option<&str> {
    let object = node.as_object()?;
    if object
        .keys()
        .any(|key| !["$ref", "description", "$defs"].contains(&key.as_str()))
    {
        return None;
    }
    object.get("$ref")?.as_str()?.strip_prefix("#/$defs/")
}

/// `path` with the tag of a variant, where it has one.
fn tagged_path(path: &str, tag: &[(String, String)]) -> String {
    if tag.is_empty() {
        return path.to_string();
    }
    let named: Vec<String> = tag
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect();
    format!("{path}[{}]", named.join(","))
}
