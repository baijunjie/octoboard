//! Why a content search's query or file mask does not compile, told as a reason from a closed set
//! and, for a regular expression, where in the query it fails (see `invalid_search_pattern` and
//! `invalid_file_mask` in `apps/daemon/PROTOCOL.md`). The client words the reason in its own
//! language: the matcher's English explanation travels only as `detail`, for diagnosis.

use regex_syntax::ast::{self, parse::Parser};
use regex_syntax::hir::{self, translate::TranslatorBuilder};

use crate::protocol::{error_code, CodedError};

/// Why a query, read as a regular expression, does not compile: the `cause` of an
/// `invalid_search_pattern`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternCause {
    /// A `(` with no `)` closing it.
    UnclosedGroup,
    /// A `)` with no `(` opening it.
    UnopenedGroup,
    /// A `[` with no `]` closing it.
    UnclosedClass,
    /// A repetition with nothing to repeat (`*a`), or a `{}` count that is not one.
    InvalidRepetition,
    /// A `\` escape that is not one, or names a class that does not exist.
    InvalidEscape,
    /// A look-around or a backreference, which the syntax does not have.
    Unsupported,
    /// A `\n` the pattern must match, which a line-by-line search never can.
    LineBreak,
    Other,
}

impl PatternCause {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UnclosedGroup => "unclosed_group",
            Self::UnopenedGroup => "unopened_group",
            Self::UnclosedClass => "unclosed_class",
            Self::InvalidRepetition => "invalid_repetition",
            Self::InvalidEscape => "invalid_escape",
            Self::Unsupported => "unsupported",
            Self::LineBreak => "line_break",
            Self::Other => "other",
        }
    }
}

/// Why a glob of a file mask does not compile: the `cause` of an `invalid_file_mask`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaskCause {
    /// A `[` with no `]` closing it.
    UnclosedClass,
    /// A `{` with no `}` closing it, often one split by the comma that separates globs.
    UnclosedAlternation,
    /// A `}` with no `{` opening it.
    UnopenedAlternation,
    /// A range in a `[...]` whose start comes after its end.
    InvalidRange,
    Other,
}

impl MaskCause {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UnclosedClass => "unclosed_class",
            Self::UnclosedAlternation => "unclosed_alternation",
            Self::UnopenedAlternation => "unopened_alternation",
            Self::InvalidRange => "invalid_range",
            Self::Other => "other",
        }
    }
}

/// The refusal of `text`, which `grep-regex` would not build into a matcher with `err`.
pub fn invalid_pattern(
    text: &str,
    regex: bool,
    case_insensitive: bool,
    err: &grep_regex::Error,
) -> anyhow::Error {
    let detail = err.to_string();
    let (cause, offset) = classify_pattern(text, regex, case_insensitive, err);
    let offset = offset.map(|offset| offset.to_string());
    let mut params = vec![("cause", cause.as_str()), ("detail", detail.as_str())];
    if let Some(offset) = &offset {
        params.push(("offset", offset));
    }
    CodedError::raised(
        error_code::INVALID_SEARCH_PATTERN,
        format!("the search pattern is not a valid regular expression: {detail}"),
        &params,
    )
}

/// The refusal of `glob`, a glob of a file mask that `globset` would not compile with `err`.
pub fn invalid_mask(glob: &str, err: &globset::Error) -> anyhow::Error {
    let detail = err.kind().to_string();
    let cause = classify_mask(err.kind());
    CodedError::raised(
        error_code::INVALID_FILE_MASK,
        format!("{glob} is not a valid file mask: {detail}"),
        &[
            ("glob", glob),
            ("cause", cause.as_str()),
            ("detail", &detail),
        ],
    )
}

/// Why `text` did not build, and the UTF-16 offset into it of where, when that is known. The
/// query is parsed again on its own, as `grep-regex` parses it, for an error that carries its
/// span in the query rather than in the pattern `grep-regex` wraps it in.
pub fn classify_pattern(
    text: &str,
    regex: bool,
    case_insensitive: bool,
    err: &grep_regex::Error,
) -> (PatternCause, Option<usize>) {
    if matches!(err.kind(), grep_regex::ErrorKind::NotAllowed(_)) {
        return (PatternCause::LineBreak, None);
    }
    if !regex {
        return (PatternCause::Other, None);
    }
    let utf16 = |byte: usize| text[..byte].encode_utf16().count();
    let ast = match Parser::new().parse(text) {
        Ok(ast) => ast,
        Err(err) => return (ast_cause(err.kind()), Some(utf16(err.span().start.offset))),
    };
    match TranslatorBuilder::new()
        .utf8(false)
        .case_insensitive(case_insensitive)
        .build()
        .translate(text, &ast)
    {
        Ok(_) => (PatternCause::Other, None),
        Err(err) => (hir_cause(err.kind()), Some(utf16(err.span().start.offset))),
    }
}

fn ast_cause(kind: &ast::ErrorKind) -> PatternCause {
    use ast::ErrorKind as K;
    match kind {
        K::GroupUnclosed => PatternCause::UnclosedGroup,
        K::GroupUnopened => PatternCause::UnopenedGroup,
        K::ClassUnclosed => PatternCause::UnclosedClass,
        K::RepetitionMissing
        | K::RepetitionCountInvalid
        | K::RepetitionCountDecimalEmpty
        | K::RepetitionCountUnclosed
        | K::DecimalEmpty
        | K::DecimalInvalid => PatternCause::InvalidRepetition,
        K::EscapeHexEmpty
        | K::EscapeHexInvalid
        | K::EscapeHexInvalidDigit
        | K::EscapeUnexpectedEof
        | K::EscapeUnrecognized
        | K::ClassEscapeInvalid
        | K::UnicodeClassInvalid
        | K::SpecialWordBoundaryUnclosed
        | K::SpecialWordBoundaryUnrecognized => PatternCause::InvalidEscape,
        K::UnsupportedBackreference | K::UnsupportedLookAround => PatternCause::Unsupported,
        _ => PatternCause::Other,
    }
}

fn hir_cause(kind: &hir::ErrorKind) -> PatternCause {
    use hir::ErrorKind as K;
    match kind {
        K::UnicodePropertyNotFound
        | K::UnicodePropertyValueNotFound
        | K::UnicodePerlClassNotFound => PatternCause::InvalidEscape,
        _ => PatternCause::Other,
    }
}

fn classify_mask(kind: &globset::ErrorKind) -> MaskCause {
    use globset::ErrorKind as K;
    match kind {
        K::UnclosedClass => MaskCause::UnclosedClass,
        K::UnclosedAlternates => MaskCause::UnclosedAlternation,
        K::UnopenedAlternates => MaskCause::UnopenedAlternation,
        K::InvalidRange(..) => MaskCause::InvalidRange,
        _ => MaskCause::Other,
    }
}
