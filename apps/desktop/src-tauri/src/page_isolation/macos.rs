//! A second barrier under the UI's own rule that nothing it draws loads from the network: the
//! window's webview is created with a WebKit content rule list that blocks `http(s)` requests except
//! image loads. A report page is model-authored, but it is a component tree the UI renders with its
//! own code, which writes no HTML of the page's and shows a page's image only from a `data:` URL
//! (`docs/product/report-panel.md`'s "What a page may contain, and what it cannot do" section). This
//! list keeps any non-image `http(s)` load closed should the UI ever start one by mistake. Resource
//! hints (`<link rel="preconnect">`, `rel="dns-prefetch"`) are channels no content security policy
//! governs in WebKit, and whether WebKit consults this list before a `preconnect` is not established
//! (WebView2's request filter was measured not stopping one); a report page cannot produce one, as it
//! carries no HTML. It does not cover WebRTC: ICE traffic is not a request a content rule sees. The
//! application loads remote images in Markdown documents; its assets use `tauri://` and its daemon
//! connection uses `ws://127.0.0.1` and `ipc://`, none of which the rule matches.
//!
//! Everything here is public WebKit API (`WKContentRuleList`), so it stays within the App Store's
//! rules.

use std::cell::RefCell;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::MainThreadMarker;
use objc2_foundation::{NSError, NSString};
use objc2_web_kit::{WKContentRuleList, WKContentRuleListStore, WKWebViewConfiguration};

use crate::diagnostics::diagnostic;
use crate::MainWindowBuilder;

const RULE_LIST_IDENTIFIER: &str = "octoboard-no-web";

/// HTTP(S) images are allowed for Markdown documents; other web requests remain blocked. A dev
/// build also loads its UI from the Vite dev server, so it allows `localhost` for those resources.
#[cfg(not(debug_assertions))]
const RULES: &str = r#"[
    {"trigger":{"url-filter":"^https?://"},"action":{"type":"block"}},
    {"trigger":{"url-filter":"^https?://","resource-type":["image"]},"action":{"type":"ignore-previous-rules"}}
]"#;
#[cfg(debug_assertions)]
const RULES: &str = r#"[
    {"trigger":{"url-filter":"^https?://"},"action":{"type":"block"}},
    {"trigger":{"url-filter":"^https?://","resource-type":["image"]},"action":{"type":"ignore-previous-rules"}},
    {"trigger":{"url-filter":"^https?://localhost[:/]"},"action":{"type":"ignore-previous-rules"}}
]"#;

/// Builds the webview configuration the main window is created with, with the rule list on it, and
/// hands it to `then`. A failure to compile the list is logged and `then` still gets a
/// configuration without it: the sanitizer is the primary barrier, and a window that does not open
/// is worse.
///
/// Compiling is asynchronous, so `then` runs from the compile's completion handler, which WebKit
/// calls on the main thread. The window has to be created from there rather than after waiting
/// for it: a rule list added to a webview that is already loading does not apply to what it has
/// started loading, and blocking the main thread to wait would stall the very run loop the
/// compile reports back on. The list is compiled on every launch instead of looked up from an
/// earlier one, which costs milliseconds for one rule and means a changed rule takes effect with
/// the build that ships it.
pub fn with_configuration(
    mtm: MainThreadMarker,
    then: impl FnOnce(Retained<WKWebViewConfiguration>) + 'static,
) {
    // SAFETY: plain allocation on the main thread `mtm` proves.
    let configuration = unsafe { WKWebViewConfiguration::new(mtm) };
    let Some(store) = (unsafe { WKContentRuleListStore::defaultStore(mtm) }) else {
        diagnostic!("no WebKit content rule list store; report pages are not isolated.");
        return then(configuration);
    };
    // The block is called once, but the type it must have is `Fn`.
    let then = RefCell::new(Some(then));
    let block = RcBlock::new(move |list: *mut WKContentRuleList, error: *mut NSError| {
        // SAFETY: WebKit passes either a valid object or null for each of the two.
        match unsafe { list.as_ref() } {
            Some(list) => unsafe {
                configuration
                    .userContentController()
                    .addContentRuleList(list)
            },
            None => {
                let reason = unsafe { error.as_ref() }
                    .map(|error| error.localizedDescription().to_string())
                    .unwrap_or_default();
                diagnostic!(
                    "cannot compile the content rule list ({reason}); report pages \
                     rely on the sanitizer alone."
                );
            }
        }
        if let Some(then) = then.borrow_mut().take() {
            then(configuration.clone());
        }
    });
    // SAFETY: both strings are valid, and the block is copied by WebKit if it outlives this call.
    unsafe {
        store.compileContentRuleListForIdentifier_encodedContentRuleList_completionHandler(
            Some(&NSString::from_str(RULE_LIST_IDENTIFIER)),
            Some(&NSString::from_str(RULES)),
            Some(&block),
        );
    }
}

/// What the window's builder is given: the webview configuration `with_configuration` prepared.
pub type Configuration = Retained<WKWebViewConfiguration>;

/// `with_configuration`, called on the main thread.
pub fn prepare(then: impl FnOnce(Configuration) + 'static) {
    let mtm = MainThreadMarker::new().expect("this runs on the main thread");
    with_configuration(mtm, then);
}

/// The configuration `prepare` handed over, with its content rule list on it.
pub fn apply(
    builder: MainWindowBuilder<'_>,
    configuration: Configuration,
) -> MainWindowBuilder<'_> {
    builder.with_webview_configuration(configuration)
}

/// The content rule list is on the webview from its creation, so there is nothing to install.
pub fn install(_window: &tauri::WebviewWindow) {}
