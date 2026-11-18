//! The string enums of the types the daemon sends, opened as `Received<…>` (see the module doc of
//! `bindings`).

use std::collections::{BTreeMap, BTreeSet};

use crate::protocol::ts_text::{pieces, Piece};

/// The generic type a declaration the daemon sends names a string enum with, declared at the top
/// of the rendering by [`received_declaration`].
pub(super) const RECEIVED: &str = "Received";

/// The declaration of [`RECEIVED`], with its doc comment.
pub(super) fn received_declaration() -> String {
    format!(
        "\
/**
 * A value of a string enum as the daemon sends it: one of the values of `T`, or one a newer daemon
 * of the same generation added. A client takes a value it does not know as the enum's stated
 * fallback, or shows the record and does not act on the value (`apps/daemon/PROTOCOL.md`,
 * \"Versions and compatibility\"). `T` is the closed set a client sends and can tell apart.
 */
export type {RECEIVED}<T extends string> = T | (string & {{}});
"
    )
}

/// The declarations with every string enum in a type the daemon sends rendered open, as
/// `Received<…>`: the enum's name where the type names it, and a union of string literals a field
/// narrows its type to with `#[ts(type = ...)]`. Output enums may gain values within a generation,
/// so a client that read them as closed would type-check a `switch` that must cover every value, or
/// a lookup by the raw value, that breaks on a new value.
///
/// A type the daemon sends is any type an event names, directly or through another type; which
/// types those are is read from the declarations' text, so a type named only by
/// `#[ts(as = ...)]` or `#[ts(type = ...)]` counts too. A type only the client sends keeps the
/// closed enum, so a client cannot send a value the daemon does not know; a type both send (a
/// record the client sends back as it was given) is opened like any other the daemon sends, since
/// the client holds what the daemon gave it. An enum's own declaration stays closed: it is the set
/// of values the client knows, and what a request names.
pub(super) fn open_received(
    mut bare: BTreeMap<String, String>,
    output_roots: &[String],
) -> BTreeMap<String, String> {
    assert!(
        !bare.contains_key(RECEIVED),
        "a wire type is named `{RECEIVED}`, which the rendering declares itself: rename it"
    );
    let enums: BTreeSet<String> = bare
        .iter()
        .filter(|(_, declaration)| is_string_enum(declaration))
        .map(|(name, _)| name.clone())
        .collect();
    // The types the daemon sends: the output roots and every declared type their text names.
    let mut sent: BTreeSet<String> = BTreeSet::new();
    let mut pending: Vec<String> = output_roots.to_vec();
    while let Some(name) = pending.pop() {
        let Some(declaration) = bare.get(&name) else {
            panic!("the output root `{name}` has no declaration");
        };
        if !sent.insert(name) {
            continue;
        }
        for piece in read(declaration) {
            if let Piece::Name(named) = piece {
                if bare.contains_key(named) && !sent.contains(named) {
                    pending.push(named.to_string());
                }
            }
        }
    }
    for name in sent.difference(&enums) {
        let declaration = bare.get_mut(name).expect("a sent type is declared");
        *declaration = opened(declaration, &enums);
    }
    bare
}

/// Whether a declaration is a string enum: `type X = "a" | "b";`.
pub(super) fn is_string_enum(declaration: &str) -> bool {
    let pieces: Vec<Piece> = read(declaration)
        .into_iter()
        .filter(|piece| !matches!(piece, Piece::Gap(_)))
        .collect();
    let [Piece::Name("type"), Piece::Name(_), Piece::Mark("="), members @ .., Piece::Mark(";")] =
        pieces.as_slice()
    else {
        return false;
    };
    !members.is_empty()
        && members.iter().enumerate().all(|(n, piece)| match n % 2 {
            0 => matches!(piece, Piece::Literal(_)),
            _ => *piece == Piece::Mark("|"),
        })
        && members.len() % 2 == 1
}

/// A declaration with each named enum of `enums` written `Received<Enum>`, and each union of two
/// or more string literals written `Received<"a" | "b">`. A property name, and a single literal (a
/// tagged union's discriminant), are left alone. A union whose string literals another member
/// splits (`"a" | null | "b"`, `"a" | { … } | "b"`) is refused: it would be read as single literals
/// and stay closed.
fn opened(declaration: &str, enums: &BTreeSet<String>) -> String {
    let pieces = read(declaration);
    let next = |from: usize| (from..pieces.len()).find(|&n| !matches!(pieces[n], Piece::Gap(_)));
    // A property name is followed by `:`, or by `?:` when it may be left out.
    let is_property = |at: usize| {
        matches!(
            next(at + 1).map(|n| pieces[n]),
            Some(Piece::Mark(":" | "?"))
        )
    };
    // The last piece of the union member that starts at `start`: past every bracket it opens, up to
    // the `|` after it or the end of the union (a `,`, a `;` or a bracket it did not open).
    let member_end = |start: usize| {
        let mut depth = 0usize;
        let mut end = start;
        for (n, piece) in pieces.iter().enumerate().skip(start) {
            match piece {
                Piece::Gap(_) => continue,
                Piece::Mark("{" | "(" | "[" | "<") => depth += 1,
                Piece::Mark("}" | ")" | "]" | ">") if depth == 0 => break,
                Piece::Mark("}" | ")" | "]" | ">") => depth -= 1,
                Piece::Mark("|" | "," | ";") if depth == 0 => break,
                _ => {}
            }
            end = n;
        }
        end
    };
    let mut out = String::new();
    let mut at = 0;
    while at < pieces.len() {
        match pieces[at] {
            Piece::Name(name) if enums.contains(name) && !is_property(at) => {
                out.push_str(&format!("{RECEIVED}<{name}>"));
            }
            Piece::Literal(_) if !is_property(at) => {
                // The last literal of the union that starts here.
                let mut last = at;
                while let Some(bar) = next(last + 1).filter(|&n| pieces[n] == Piece::Mark("|")) {
                    match next(bar + 1) {
                        Some(n) if matches!(pieces[n], Piece::Literal(_)) && !is_property(n) => {
                            last = n;
                        }
                        _ => break,
                    }
                }
                let text: String = pieces[at..=last].iter().map(Piece::text).collect();
                // The members after the literals, each read whole, until the union ends.
                let mut member = last;
                while let Some(bar) = next(member + 1).filter(|&n| pieces[n] == Piece::Mark("|")) {
                    match next(bar + 1) {
                        Some(n) if matches!(pieces[n], Piece::Literal(_)) && !is_property(n) => {
                            panic!(
                                "a union of string literals is split by another member after \
                                 `{text}`: put its literals together, before `null` or any type, \
                                 so that they are opened as one `{RECEIVED}<…>`"
                            )
                        }
                        Some(n) => member = member_end(n),
                        None => break,
                    }
                }
                if last == at {
                    out.push_str(&text);
                } else {
                    out.push_str(&format!("{RECEIVED}<{text}>"));
                }
                at = last;
            }
            piece => out.push_str(piece.text()),
        }
        at += 1;
    }
    out
}

/// The pieces of a declaration `ts-rs` wrote, which always reads.
fn read(declaration: &str) -> Vec<Piece<'_>> {
    pieces(declaration)
        .unwrap_or_else(|error| panic!("cannot read the declaration `{declaration}`: {error}"))
}

fn map(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
    entries
        .iter()
        .map(|(name, declaration)| (name.to_string(), declaration.to_string()))
        .collect()
}

fn names(entries: &[&str]) -> BTreeSet<String> {
    entries.iter().map(|name| name.to_string()).collect()
}

#[test]
fn a_string_enum_is_told_from_the_other_declarations() {
    for declaration in [
        "type X = \"a\";",
        "type X = \"a\" | \"b\";",
        "type X = \"a\" | /* two */ \"b\" | \"c\";",
    ] {
        assert!(is_string_enum(declaration), "{declaration}");
    }
    for declaration in [
        "type X = { a: \"b\" };",
        "type X = \"a\" | B;",
        "type X = \"a\" \"b\";",
        "type X = \"a\" | ;",
        "type X = | \"a\";",
        "type X = ;",
        "type X = \"a\" | \"b\"",
        "type X = Array<\"a\">;",
    ] {
        assert!(!is_string_enum(declaration), "{declaration}");
    }
}

#[test]
fn a_named_enum_is_opened_at_each_position_and_a_property_of_that_name_is_not() {
    let enums = names(&["Foo"]);
    assert_eq!(
        opened(
            "type A = { a: Foo, b?: Foo | null, c: Array<Foo>, Foo: string, d?: Foo, };",
            &enums
        ),
        "type A = { a: Received<Foo>, b?: Received<Foo> | null, c: Array<Received<Foo>>, Foo: string, d?: Received<Foo>, };"
    );
    // A name that is not an enum of the set is left alone.
    assert_eq!(
        opened("type A = { a: Bar, };", &enums),
        "type A = { a: Bar, };"
    );
}

#[test]
fn a_union_of_string_literals_is_opened_as_a_whole_and_a_property_key_is_not() {
    let enums = names(&[]);
    assert_eq!(
        opened(
            "type A = { \"type\": \"x\", k: \"a\" | \"b\" | \"c\", l?: \"d\" | \"e\" | null, };",
            &enums
        ),
        "type A = { \"type\": \"x\", k: Received<\"a\" | \"b\" | \"c\">, l?: Received<\"d\" | \"e\"> | null, };"
    );
}

#[test]
fn opening_leaves_comments_and_layout_as_they_were() {
    let enums = names(&["Foo"]);
    let declaration =
        "type A = {\n  /** Says Foo and \"a\" | \"b\". */\n  a: Foo, // Foo\n  b: \"x\" | \"y\",\n};";
    let opened = opened(declaration, &enums);
    assert_eq!(
        opened,
        "type A = {\n  /** Says Foo and \"a\" | \"b\". */\n  a: Received<Foo>, // Foo\n  b: Received<\"x\" | \"y\">,\n};"
    );
}

#[test]
fn a_single_literal_is_left_alone_whatever_follows_it() {
    let declaration =
        "type A = { \"type\": \"a\" } | { \"type\": \"b\", k: \"c\" | null, l: \"d\" | Foo, };";
    assert_eq!(opened(declaration, &names(&[])), declaration);
}

#[test]
#[should_panic(expected = "split by another member")]
fn a_union_of_string_literals_split_by_null_is_refused() {
    opened("type A = { k: \"a\" | null | \"b\", };", &names(&[]));
}

#[test]
#[should_panic(expected = "split by another member")]
fn a_union_of_string_literals_split_by_a_type_is_refused() {
    opened("type A = { k: \"a\" | \"b\" | Foo | \"c\", };", &names(&[]));
}

#[test]
#[should_panic(expected = "split by another member")]
fn a_union_of_string_literals_split_by_an_object_is_refused() {
    opened(
        "type A = \"a\" | { \"b\": { c: Array<\"d\">, }, } | \"e\";",
        &names(&[]),
    );
}

#[test]
fn a_literal_inside_a_later_member_does_not_count_as_splitting_the_union() {
    let declaration =
        "type A = { k: \"a\" | Array<{ c: \"d\", }> | [string, \"e\"], l: \"f\" | { m: \"g\" }, };";
    assert_eq!(opened(declaration, &names(&[])), declaration);
}

#[test]
#[should_panic(expected = "cannot read the declaration")]
fn an_unreadable_declaration_is_refused() {
    opened("type A = { k: \"a, };", &names(&[]));
}

#[test]
fn only_types_an_output_root_reaches_open_their_enums() {
    let bare = map(&[
        ("Event", "type Event = { a: Foo, b: Bar, c: Plain, };"),
        ("Bar", "type Bar = { f: Foo, };"),
        ("Plain", "type Plain = { n: number, };"),
        ("RequestOnly", "type RequestOnly = { f: Foo, };"),
        ("Foo", "type Foo = \"x\" | \"y\";"),
    ]);
    let result = open_received(bare, &["Event".to_string()]);
    assert_eq!(
        result["Event"],
        "type Event = { a: Received<Foo>, b: Bar, c: Plain, };"
    );
    assert_eq!(result["Bar"], "type Bar = { f: Received<Foo>, };");
    assert_eq!(result["Plain"], "type Plain = { n: number, };");
    assert_eq!(result["RequestOnly"], "type RequestOnly = { f: Foo, };");
    assert_eq!(result["Foo"], "type Foo = \"x\" | \"y\";");
}

#[test]
#[should_panic(expected = "has no declaration")]
fn an_output_root_without_a_declaration_is_refused() {
    open_received(map(&[("Foo", "type Foo = \"x\";")]), &["Event".to_string()]);
}

#[test]
#[should_panic(expected = "which the rendering declares itself")]
fn a_wire_type_named_received_is_refused() {
    open_received(
        map(&[
            ("Event", "type Event = {};"),
            ("Received", "type Received = {};"),
        ]),
        &["Event".to_string()],
    );
}
