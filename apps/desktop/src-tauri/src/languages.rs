//! The system's preferred languages for the window's query (`?languages=`), where the webview
//! cannot report them itself.

#[cfg(target_os = "macos")]
mod macos {
    /// The system's preferred languages, most preferred first, comma-separated, for the UI to
    /// choose its language from. The webview cannot tell it: WKWebView's `navigator.languages`
    /// holds only the first preference, so a user whose first language Octoboard does not offer
    /// would get the fallback even when a later preference is offered. A snapshot at launch, as
    /// macOS applications generally take one.
    pub fn preferred_languages() -> Option<String> {
        let languages = objc2_foundation::NSLocale::preferredLanguages();
        let tags: Vec<String> = languages.iter().map(|tag| tag.to_string()).collect();
        (!tags.is_empty()).then(|| tags.join(","))
    }
}
#[cfg(target_os = "macos")]
use macos as platform;

/// Every target but macOS, Linux included.
#[cfg(not(target_os = "macos"))]
mod windows {
    /// None: the UI reads the webview's own `navigator.languages`.
    pub fn preferred_languages() -> Option<String> {
        None
    }
}
#[cfg(not(target_os = "macos"))]
use windows as platform;

pub use platform::preferred_languages;
