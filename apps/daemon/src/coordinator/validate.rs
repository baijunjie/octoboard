//! The checks of the text fields requests carry: required text, lengths, paths, tags and icons.

use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::Result;

use crate::hostfs;
use crate::protocol::{error_code, CodedError};
use crate::text::trimmed_or_none;

/// A request that lacks a field its kind needs. `field` is the wire name, passed as is.
pub(super) fn field_required(field: &str) -> anyhow::Error {
    CodedError::raised(
        error_code::FIELD_REQUIRED,
        format!("`{field}` is required"),
        &[("field", field)],
    )
}

/// A name or title as stored: trimmed, and refused as a missing `field` if that leaves nothing.
pub(super) fn required_text(field: &str, text: &str) -> Result<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(field_required(field));
    }
    Ok(trimmed.to_string())
}

pub(super) const PROJECT_NAME_LIMIT: usize = 100;
pub(super) const PROJECT_DESCRIPTION_LIMIT: usize = 2_000;

pub(super) fn check_text_length(field: &str, text: &str, limit: usize) -> Result<()> {
    if text.chars().count() > limit {
        return Err(CodedError::raised(
            error_code::FIELD_TOO_LONG,
            format!("`{field}` must be at most {limit} characters"),
            &[("field", field), ("limit", &limit.to_string())],
        ));
    }
    Ok(())
}

/// What the user typed for a project's directory, as the absolute, lexically normalised path that is
/// stored. A relative path is refused: it would mean a different directory whenever the daemon's own
/// working directory differed, and nothing that compares project paths — the trusted directories
/// among them — could read it.
pub(crate) fn absolute_path(text: &str) -> Result<PathBuf> {
    let path = hostfs::expand(text);
    if !path.is_absolute() {
        return Err(CodedError::raised(
            error_code::PATH_NOT_ABSOLUTE,
            format!("`{text}` is not an absolute path; give the full path or start it with `~/`"),
            &[("path", text)],
        ));
    }
    Ok(hostfs::lexically_normalise(&path))
}

/// A project's or an agent's tags as stored: each trimmed, the empty ones dropped, and a later tag
/// dropped when an earlier one matches it ignoring case (the first spelling wins). The order is the
/// user's.
pub(super) fn normalise_tags(tags: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    tags.into_iter()
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty() && seen.insert(tag.to_lowercase()))
        .collect()
}

/// The longest avatar `data:` URL a console may carry. The UI sends a 128x128 image, which is
/// far smaller; the bound is there because the value goes to every client with each console.
const MAX_ICON_BYTES: usize = 256 * 1024;

/// A console's avatar as it is stored. Blank means unset. Checked lightly: it has to be an image
/// `data:` URL and not too long; whether it decodes is left to the client that draws it.
pub(super) fn normalize_icon(text: Option<&str>) -> Result<Option<String>> {
    let Some(text) = trimmed_or_none(text) else {
        return Ok(None);
    };
    if !text.starts_with("data:image/") {
        return Err(CodedError::raised(
            error_code::ICON_NOT_AN_IMAGE,
            "the console avatar must be an image `data:` URL",
            &[],
        ));
    }
    if text.len() > MAX_ICON_BYTES {
        return Err(CodedError::raised(
            error_code::ICON_TOO_LARGE,
            format!(
                "the console avatar is larger than {} KiB",
                MAX_ICON_BYTES / 1024
            ),
            &[("limit_kib", &(MAX_ICON_BYTES / 1024).to_string())],
        ));
    }
    Ok(Some(text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths;
    use std::path::PathBuf;

    #[test]
    fn tags_are_trimmed_and_deduplicated_ignoring_case_keeping_the_first_spelling() {
        let tags = |tags: &[&str]| normalise_tags(tags.iter().map(|tag| tag.to_string()).collect());
        assert_eq!(
            tags(&["  Rust ", "", "   ", "backend", "rust", "BACKEND", "web"]),
            ["Rust", "backend", "web"]
        );
        assert!(tags(&[]).is_empty());
    }

    #[test]
    fn a_console_icon_is_an_image_data_url_of_bounded_size() {
        assert_eq!(normalize_icon(None).unwrap(), None);
        assert_eq!(normalize_icon(Some("  ")).unwrap(), None);
        assert_eq!(
            normalize_icon(Some(" data:image/webp;base64,AAAA ")).unwrap(),
            Some("data:image/webp;base64,AAAA".to_string())
        );
        assert!(normalize_icon(Some("https://example.com/a.png")).is_err());
        assert!(normalize_icon(Some("data:text/html;base64,AAAA")).is_err());
        let huge = format!("data:image/png;base64,{}", "A".repeat(MAX_ICON_BYTES));
        assert!(normalize_icon(Some(&huge)).is_err());
    }

    /// A project's path is stored absolute and written one way, so the trusted directories can be
    /// compared against it; a relative one is refused.
    #[test]
    fn a_project_path_is_stored_absolute_and_normalised() {
        assert_eq!(
            absolute_path("/work/a/../project/./").unwrap(),
            PathBuf::from("/work/project")
        );
        assert_eq!(
            absolute_path("~/code/app/").unwrap(),
            paths::home_dir().join("code/app")
        );
        let err = absolute_path("work/project").expect_err("relative");
        assert!(err.to_string().contains("not an absolute path"), "{err}");
        assert!(absolute_path("").is_err());
    }
}
