//! Reads the crate's source into the edges the rule in the parent module checks: the module tree
//! walked from `main.rs`, each production file with its comments and literals blanked, tokenised,
//! and its `use` trees and paths read off the tokens. No parser crate: the rule needs paths only,
//! and a lexical scan reads code that does not yet compile the same way it reads the rest.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// The words that open an item, whose `#[cfg(test)]` ends with the item rather than at a comma.
const ITEM_KEYWORDS: &[&str] = &[
    "async",
    "const",
    "enum",
    "extern",
    "fn",
    "impl",
    "let",
    "macro_rules",
    "mod",
    "static",
    "struct",
    "trait",
    "type",
    "union",
    "unsafe",
    "use",
];

/// One path into another top-level module, found in production code.
pub(super) struct Edge {
    /// The file, relative to the crate's `src`.
    pub(super) file: String,
    pub(super) line: usize,
    /// The top-level module the file belongs to.
    pub(super) from: String,
    /// The top-level module the path names, or an item of the crate root (`APP_NAME`).
    pub(super) to: String,
    /// The path after `crate::<to>`, empty when the path names the module itself.
    pub(super) path: Vec<String>,
}

pub(super) struct Scan {
    /// The top-level modules production code is built from.
    pub(super) modules: BTreeSet<String>,
    pub(super) edges: Vec<Edge>,
}

/// Walks the module tree from `main.rs` along its `mod` declarations, so a file counts as
/// production code exactly when the compiler builds it outside tests, and fails on any `.rs` file
/// under `src` the walk did not reach, which would otherwise go unchecked.
pub(super) fn scan_crate() -> Scan {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let root = parse(&read(&src.join("main.rs")), 0);
    let modules = root
        .children
        .iter()
        .filter(|c| !c.test_only)
        .map(|c| c.name.clone())
        .collect();
    let mut reached = BTreeSet::from([src.join("main.rs")]);
    let mut edges = Vec::new();
    let mut pending: Vec<(PathBuf, Vec<String>, bool)> = root
        .children
        .iter()
        .map(|c| (child_file(&src, &c.name), vec![c.name.clone()], c.test_only))
        .collect();
    while let Some((file, module, test_only)) = pending.pop() {
        reached.insert(file.clone());
        let parsed = parse(&read(&file), module.len());
        let dir = if file.file_name().is_some_and(|n| n == "mod.rs") {
            file.parent().unwrap().to_path_buf()
        } else {
            file.with_extension("")
        };
        for child in &parsed.children {
            let mut path = module.clone();
            path.push(child.name.clone());
            pending.push((
                child_file(&dir, &child.name),
                path,
                test_only || child.test_only,
            ));
        }
        if test_only {
            continue;
        }
        let rel = file.strip_prefix(&src).unwrap().display().to_string();
        for found in parsed.refs {
            if let Some((to, rest)) = found.path.split_first() {
                edges.push(Edge {
                    file: rel.clone(),
                    line: found.line,
                    from: module[0].clone(),
                    to: to.clone(),
                    path: rest.to_vec(),
                });
            }
        }
    }
    let mut unreached = Vec::new();
    collect_rs(&src, &mut |file| {
        if !reached.contains(file) {
            unreached.push(file.strip_prefix(&src).unwrap().display().to_string());
        }
    });
    assert!(
        unreached.is_empty(),
        "the dependency-direction scan did not reach these files through `mod` declarations from \
         main.rs, so it cannot tell whether they are production code: {unreached:?}"
    );
    Scan { modules, edges }
}

fn read(file: &Path) -> String {
    std::fs::read_to_string(file).unwrap_or_else(|e| panic!("reading {}: {e}", file.display()))
}

fn child_file(dir: &Path, name: &str) -> PathBuf {
    let flat = dir.join(format!("{name}.rs"));
    if flat.exists() {
        flat
    } else {
        dir.join(name).join("mod.rs")
    }
}

fn collect_rs(dir: &Path, found: &mut dyn FnMut(&Path)) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_rs(&path, found);
        } else if path.extension().is_some_and(|e| e == "rs") {
            found(&path);
        }
    }
}

/// Replaces comments and the contents of string and character literals with spaces, keeping
/// newlines so line numbers survive.
fn blank_non_code(src: &str) -> String {
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let blank = |out: &mut String, c: char| out.push(if c == '\n' { '\n' } else { ' ' });
    // Where a raw string opens at `i` (`r"`, `r#"`, `br"`, …): its `#` count and opening quote.
    let raw_string = |i: usize| {
        let start = match (chars[i], chars.get(i + 1)) {
            ('r', _) => i + 1,
            ('b', Some('r')) => i + 2,
            _ => return None,
        };
        let hashes = chars[start..].iter().take_while(|&&h| h == '#').count();
        (chars.get(start + hashes) == Some(&'"')).then_some((hashes, start + hashes))
    };
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        let after_ident = i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '_');
        if c == '/' && next == Some('/') {
            while i < chars.len() && chars[i] != '\n' {
                blank(&mut out, chars[i]);
                i += 1;
            }
        } else if c == '/' && next == Some('*') {
            let mut depth = 0;
            while i < chars.len() {
                if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
                    depth += 1;
                    out.push_str("  ");
                    i += 2;
                } else if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    out.push_str("  ");
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    blank(&mut out, chars[i]);
                    i += 1;
                }
            }
        } else if let Some((hashes, open)) = raw_string(i).filter(|_| !after_ident) {
            out.extend(&chars[i..=open]);
            i = open + 1;
            while i < chars.len() {
                let closes = chars[i] == '"'
                    && chars[i + 1..]
                        .iter()
                        .take(hashes)
                        .filter(|&&h| h == '#')
                        .count()
                        == hashes;
                if closes {
                    out.push('"');
                    out.push_str(&"#".repeat(hashes));
                    i += 1 + hashes;
                    break;
                }
                blank(&mut out, chars[i]);
                i += 1;
            }
        } else if c == '"' {
            out.push('"');
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                if chars[i] == '\\' {
                    blank(&mut out, chars[i]);
                    i += 1;
                }
                if i < chars.len() {
                    blank(&mut out, chars[i]);
                    i += 1;
                }
            }
            out.push('"');
            i += 1;
        } else if c == '\'' {
            // A character literal is `'x'` or an escape; anything else is a lifetime or a label.
            let close = if next == Some('\\') {
                // The escape is at least two characters, so `'\''` closes on its second quote.
                chars[i + 3..]
                    .iter()
                    .position(|&k| k == '\'')
                    .map(|p| i + 3 + p)
            } else if chars.get(i + 2) == Some(&'\'') {
                Some(i + 2)
            } else {
                None
            };
            out.push('\'');
            match close {
                Some(close) => {
                    for &k in &chars[i + 1..close] {
                        blank(&mut out, k);
                    }
                    out.push('\'');
                    i = close + 1;
                }
                None => i += 1,
            }
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Ident(String),
    /// `::`
    Path,
    /// `->` and `=>`, kept whole so their `>` is not read as a closing angle bracket.
    Arrow,
    Punct(char),
}

fn tokenize(code: &str) -> Vec<(Tok, usize)> {
    let chars: Vec<char> = code.chars().collect();
    let mut toks = Vec::new();
    let mut line = 1;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\n' {
            line += 1;
            i += 1;
        } else if c.is_whitespace() {
            i += 1;
        } else if c.is_alphabetic() || c == '_' {
            // `r#name` is the identifier `name`.
            if c == 'r' && chars.get(i + 1) == Some(&'#') {
                i += 2;
            }
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            toks.push((Tok::Ident(chars[start..i].iter().collect()), line));
        } else if c.is_ascii_digit() {
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
        } else if c == ':' && chars.get(i + 1) == Some(&':') {
            toks.push((Tok::Path, line));
            i += 2;
        } else if (c == '-' || c == '=') && chars.get(i + 1) == Some(&'>') {
            toks.push((Tok::Arrow, line));
            i += 2;
        } else {
            toks.push((Tok::Punct(c), line));
            i += 1;
        }
    }
    toks
}

struct Child {
    name: String,
    test_only: bool,
}

/// A path that leaves the file's top-level module: its segments from the crate root on.
struct Found {
    path: Vec<String>,
    line: usize,
}

/// A name a `use` brings in from outside the file's top-level module.
struct Import {
    name: String,
    /// What it names, from the crate root on.
    path: Vec<String>,
    line: usize,
}

struct Parsed {
    children: Vec<Child>,
    refs: Vec<Found>,
}

/// Reads one file. `depth` is how many modules deep the file sits (0 for `main.rs`), which decides
/// whether a run of `super::` climbs out of its top-level module.
fn parse(src: &str, depth: usize) -> Parsed {
    let toks = tokenize(&blank_non_code(src));
    let mut p = Parser {
        toks: &toks,
        at: 0,
        depth,
        braces: Vec::new(),
        children: Vec::new(),
        refs: Vec::new(),
        imports: Vec::new(),
        relative: Vec::new(),
    };
    p.run();
    // A path starting with an imported name continues the import's path. An imported module seen
    // through such paths is left to them, so an edge carries the item it reaches.
    let mut refs = p.refs;
    let mut followed = BTreeSet::new();
    for (path, line) in p.relative {
        if let Some(import) = p.imports.iter().find(|i| i.name == path[0]) {
            followed.insert(import.name.clone());
            let mut full = import.path.clone();
            full.extend_from_slice(&path[1..]);
            refs.push(Found { path: full, line });
        }
    }
    for import in p.imports {
        let module = import
            .path
            .last()
            .is_some_and(|s| s.starts_with(|c: char| c.is_lowercase()));
        if !(module && followed.contains(&import.name)) {
            refs.push(Found {
                path: import.path,
                line: import.line,
            });
        }
    }
    Parsed {
        children: p.children,
        refs,
    }
}

/// What an open `{` belongs to.
enum Brace {
    /// An inline `mod`, which `super::` climbs out of.
    Module,
    /// The body of a `struct`, `enum` or `union`, or of an enum variant, whose fields are types.
    Definition,
    Other,
}

struct Parser<'a> {
    toks: &'a [(Tok, usize)],
    at: usize,
    depth: usize,
    /// One entry per open `{`, saying what it opened.
    braces: Vec<Brace>,
    children: Vec<Child>,
    refs: Vec<Found>,
    imports: Vec<Import>,
    /// Paths starting with a plain name, to be matched against `imports` once the file is read.
    relative: Vec<(Vec<String>, usize)>,
}

impl Parser<'_> {
    fn tok(&self, at: usize) -> Option<&Tok> {
        self.toks.get(at).map(|(t, _)| t)
    }

    fn ident(&self, at: usize) -> Option<&str> {
        match self.tok(at) {
            Some(Tok::Ident(word)) => Some(word),
            _ => None,
        }
    }

    fn is(&self, at: usize, c: char) -> bool {
        self.tok(at) == Some(&Tok::Punct(c))
    }

    fn line(&self) -> usize {
        self.toks.get(self.at).map_or(0, |(_, l)| *l)
    }

    fn run(&mut self) {
        while self.at < self.toks.len() {
            if self.is(self.at, '#') {
                let inner = self.is(self.at + 1, '!');
                let test_only = self.attributes();
                if test_only && inner {
                    // `#![cfg(test)]` makes the whole file test code.
                    self.refs.clear();
                    self.imports.clear();
                    self.relative.clear();
                    self.children.clear();
                    return;
                }
                if test_only {
                    self.skip_item();
                }
                continue;
            }
            let starts_path = self.at == 0
                || (self.tok(self.at - 1) != Some(&Tok::Path) && !self.is(self.at - 1, '.'));
            match self.tok(self.at).cloned() {
                Some(Tok::Ident(word)) if word == "mod" => self.module(false),
                Some(Tok::Ident(word)) if word == "use" => {
                    let line = self.line();
                    self.at += 1;
                    let mut paths = Vec::new();
                    self.use_tree(Vec::new(), &mut paths);
                    for (path, name) in paths {
                        if let Some(path) = self.rooted(&path) {
                            match name {
                                Some(name) => self.imports.push(Import { name, path, line }),
                                None => self.refs.push(Found { path, line }),
                            }
                        }
                    }
                }
                Some(Tok::Ident(word))
                    if starts_path && self.tok(self.at + 1) == Some(&Tok::Path) =>
                {
                    let line = self.line();
                    let path = self.path();
                    if word == "crate" || word == "super" {
                        if let Some(path) = self.rooted(&path) {
                            self.refs.push(Found { path, line });
                        }
                    } else if word != "self" && word != "Self" && path.len() > 1 {
                        self.relative.push((path, line));
                    }
                }
                Some(Tok::Punct('{')) => {
                    let brace = self.brace_at(self.at);
                    self.braces.push(brace);
                    self.at += 1;
                }
                Some(Tok::Punct('}')) => {
                    self.braces.pop();
                    self.at += 1;
                }
                _ => self.at += 1,
            }
        }
    }

    /// Consumes a run of attributes, telling whether one of them limits the item to tests.
    fn attributes(&mut self) -> bool {
        let mut test_only = false;
        while self.is(self.at, '#') {
            self.at += 1;
            if self.is(self.at, '!') {
                self.at += 1;
            }
            if !self.is(self.at, '[') {
                return test_only;
            }
            let start = self.at;
            self.skip_group();
            if self.ident(start + 1) == Some("cfg") && self.is(start + 2, '(') {
                let mut at = start + 3;
                test_only |= self.only_in_tests(&mut at);
            }
        }
        test_only
    }

    /// Reads one `cfg` predicate from `at` on, telling whether it holds only in a test build:
    /// `test` itself, an `all(…)` with such a predicate in it, or an `any(…)` of nothing else.
    /// `not(…)` and every other option can hold outside tests.
    fn only_in_tests(&self, at: &mut usize) -> bool {
        let Some(word) = self.ident(*at) else {
            *at += 1;
            return false;
        };
        *at += 1;
        if !self.is(*at, '(') {
            // An option such as `unix` or `target_os = "…"` (the string is blanked to `" "`).
            if self.is(*at, '=') {
                *at += 3;
            }
            return word == "test";
        }
        *at += 1;
        let mut found = Vec::new();
        while *at < self.toks.len() && !self.is(*at, ')') {
            found.push(self.only_in_tests(at));
            if self.is(*at, ',') {
                *at += 1;
            }
        }
        *at += 1;
        match word {
            "all" => found.iter().any(|&f| f),
            "any" => !found.is_empty() && found.iter().all(|&f| f),
            _ => false,
        }
    }

    /// What the `{` at `at` opens: an enum variant's body inside a definition, or the body of a
    /// definition when a `struct`, `enum` or `union` keyword comes since the last `;`, `{` or `}`
    /// outside brackets (so the `;` of `[u8; 4]` in a bound does not cut the look short).
    fn brace_at(&self, at: usize) -> Brace {
        if matches!(self.braces.last(), Some(Brace::Definition)) {
            return Brace::Definition;
        }
        let mut depth = 0i32;
        let mut k = at;
        while k > 0 {
            k -= 1;
            match self.tok(k) {
                Some(Tok::Punct(')' | ']')) => depth += 1,
                Some(Tok::Punct('(' | '[')) => depth -= 1,
                Some(Tok::Punct(';' | '{' | '}')) if depth <= 0 => break,
                Some(Tok::Ident(word))
                    if matches!(word.as_str(), "struct" | "enum" | "union")
                        && self.ident(k + 1).is_some()
                        && (k == 0
                            || (self.tok(k - 1) != Some(&Tok::Path) && !self.is(k - 1, '.'))) =>
                {
                    return Brace::Definition;
                }
                _ => {}
            }
        }
        Brace::Other
    }

    /// Steps over a balanced `(…)`, `[…]` or `{…}` starting at the current token.
    fn skip_group(&mut self) {
        let mut depth = 0i32;
        while let Some(tok) = self.tok(self.at).cloned() {
            self.at += 1;
            match tok {
                Tok::Punct('(' | '[' | '{') => depth += 1,
                Tok::Punct(')' | ']' | '}') => depth -= 1,
                _ => {}
            }
            if depth <= 0 {
                return;
            }
        }
    }

    /// Steps over the item, statement or field an attribute was attached to.
    fn skip_item(&mut self) {
        if self.ident(self.at) == Some("pub") {
            self.at += 1;
            if self.is(self.at, '(') {
                self.skip_group();
            }
        }
        if self.ident(self.at) == Some("mod") {
            self.module(true);
            return;
        }
        // An item ends at its `;` or its closing brace. Anything else an attribute sits on (an enum
        // variant, a match arm, a field of a struct or of a struct expression, a statement) also
        // ends at a comma outside brackets, and a field's type (`name: T`) in a definition may hold
        // commas inside angle brackets; in a struct expression a `<` is a comparison.
        let item = self
            .ident(self.at)
            .is_some_and(|word| ITEM_KEYWORDS.contains(&word));
        let field = !item
            && self.ident(self.at).is_some()
            && self.is(self.at + 1, ':')
            && matches!(self.braces.last(), Some(Brace::Definition));
        let mut depth = 0i32;
        let mut angle = 0i32;
        while let Some(tok) = self.tok(self.at).cloned() {
            match tok {
                Tok::Punct('(' | '[' | '{') => depth += 1,
                Tok::Punct(')' | ']' | '}') => {
                    depth -= 1;
                    if depth < 0 {
                        return;
                    }
                }
                Tok::Punct('<') if field => angle += 1,
                Tok::Punct('>') if field => angle -= 1,
                _ => {}
            }
            self.at += 1;
            match tok {
                Tok::Punct(';') if depth == 0 => return,
                Tok::Punct(',') if !item && depth == 0 && angle == 0 => return,
                Tok::Punct('}') if depth == 0 => return,
                _ => {}
            }
        }
    }

    /// At `mod`: records a file module, or enters an inline one (skipping it when test-only).
    fn module(&mut self, test_only: bool) {
        let Some(name) = self.ident(self.at + 1).map(str::to_owned) else {
            self.at += 1;
            return;
        };
        if self.is(self.at + 2, ';') {
            self.children.push(Child { name, test_only });
            self.at += 3;
        } else if self.is(self.at + 2, '{') {
            self.at += 2;
            if test_only {
                self.skip_group();
            } else {
                self.braces.push(Brace::Module);
                self.at += 1;
            }
        } else {
            self.at += 1;
        }
    }

    /// Reads `a::b::c` from the current token, stopping before anything that is not a segment.
    fn path(&mut self) -> Vec<String> {
        let mut segments = Vec::new();
        while let Some(word) = self.ident(self.at).map(str::to_owned) {
            segments.push(word);
            self.at += 1;
            if self.tok(self.at) == Some(&Tok::Path) && self.ident(self.at + 1).is_some() {
                self.at += 1;
            } else {
                break;
            }
        }
        segments
    }

    /// Reads a `use` tree, expanding `{…}` groups into one path per leaf, each with the name it
    /// brings into scope (none for a glob).
    fn use_tree(&mut self, prefix: Vec<String>, out: &mut Vec<(Vec<String>, Option<String>)>) {
        if self.tok(self.at) == Some(&Tok::Path) {
            self.at += 1;
        }
        let mut path = prefix;
        while let Some(word) = self.ident(self.at).map(str::to_owned) {
            path.push(word);
            self.at += 1;
            if self.tok(self.at) != Some(&Tok::Path) {
                break;
            }
            self.at += 1;
        }
        if self.is(self.at, '*') {
            self.at += 1;
            out.push((path, None));
        } else if self.is(self.at, '{') {
            self.at += 1;
            while self.at < self.toks.len() && !self.is(self.at, '}') {
                self.use_tree(path.clone(), out);
                if self.is(self.at, ',') {
                    self.at += 1;
                }
            }
            self.at += 1;
        } else {
            // `self` in a group names the prefix itself.
            if path.last().is_some_and(|s| s == "self") {
                path.pop();
            }
            let mut name = path.last().cloned();
            if self.ident(self.at) == Some("as") {
                name = self.ident(self.at + 1).map(str::to_owned);
                self.at += 2;
            }
            out.push((path, name));
        }
    }

    /// The path from the crate root on, when it leaves the file's top-level module through
    /// `crate::` or a run of `super::` that climbs out of it.
    fn rooted(&self, path: &[String]) -> Option<Vec<String>> {
        let rest = match path.first().map(String::as_str) {
            Some("crate") => &path[1..],
            Some("super") => {
                let climbs = path.iter().take_while(|s| *s == "super").count();
                let depth = self.depth
                    + self
                        .braces
                        .iter()
                        .filter(|b| matches!(b, Brace::Module))
                        .count();
                if climbs < depth {
                    return None;
                }
                &path[climbs..]
            }
            _ => return None,
        };
        (!rest.is_empty()).then(|| rest.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::parse;

    fn targets(src: &str) -> Vec<String> {
        let mut found: Vec<String> = parse(src, 1)
            .refs
            .into_iter()
            .map(|f| f.path.join("::"))
            .collect();
        found.sort();
        found
    }

    #[test]
    fn a_test_only_variant_arm_or_field_hides_only_itself() {
        let src = r#"
enum E {
    #[cfg(test)]
    A(crate::accounts::A),
    B(crate::server::B),
}
fn f(x: u8) -> u8 {
    match x {
        #[cfg(test)]
        0 => crate::accounts::zero(),
        _ => crate::server::other(),
    }
}
struct S {
    #[cfg(test)]
    a: std::collections::HashMap<u8, crate::accounts::Z>,
    b: crate::server::W,
}
fn h(x: u8, y: u8) -> S {
    S {
        #[cfg(test)]
        a: x < y,
        b: crate::server::expression(),
    }
}
enum V {
    A([u8; 4]),
    B {
        #[cfg(test)]
        a: std::collections::HashMap<u8, crate::accounts::V>,
        b: crate::server::variant(),
    },
}
"#;
        assert_eq!(
            targets(src),
            [
                "server::B",
                "server::W",
                "server::expression",
                "server::other",
                "server::variant",
            ]
        );
    }

    #[test]
    fn only_a_predicate_that_needs_test_hides_its_item() {
        let src = r#"
#[cfg(all(test, unix))]
use crate::accounts::under_all;
#[cfg(any(test, all(test, unix)))]
use crate::accounts::under_any;
#[cfg(all(not(test), unix))]
use crate::server::not_test;
#[cfg(any(test, unix))]
use crate::server::any_unix;
#[cfg(all(unix, target_os = "macos"))]
use crate::server::mac;
"#;
        assert_eq!(
            targets(src),
            ["server::any_unix", "server::mac", "server::not_test"]
        );
    }

    #[test]
    fn comments_literals_and_test_code_name_nothing_and_imports_are_followed() {
        let src = r##"
// crate::accounts::a
/* crate::accounts /* nested */ crate::accounts */
const T: &str = "crate::accounts::a \" crate::accounts";
const R: &str = r#"crate::accounts "quoted""#;
const C: char = '"';
#[cfg(test)]
use crate::accounts::hidden;
use crate::{
    mcp::{self as m, Owner as O},
    wire_path,
};
fn g<'a>(x: &'a str) -> &'a str {
    m::SERVER_KEY;
    super::paths::home_dir();
    x
}
"##;
        assert_eq!(
            targets(src),
            [
                "mcp::Owner",
                "mcp::SERVER_KEY",
                "paths::home_dir",
                "wire_path"
            ]
        );
    }
}
