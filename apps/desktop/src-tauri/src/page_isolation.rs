//! A second barrier under the report panel's sanitizer: the window's webview is created with a
//! WebKit content rule list that blocks every `http(s)` request. A page in the panel is
//! model-authored HTML, and the UI strips everything from it that could reach the network
//! (`packages/ui/src/report/pageBridge.js`). `<link rel="preconnect">` and `rel="dns-prefetch"` are
//! channels no content security policy governs in WebKit, so this list is what keeps them, and any
//! other `http(s)` load, closed if the sanitizer ever misses one. It does not cover WebRTC: ICE
//! traffic is not a request a content rule sees, and what backs the sanitizer up there is the
//! page's nonce-only `script-src`, under which no script of the page's own runs. The application
//! itself never needs the web: it loads `tauri://` assets and reaches the daemon over
//! `ws://127.0.0.1` and `ipc://`, none of which the rule matches.
//!
//! Everything here is public WebKit API (`WKContentRuleList`), so it stays within the App Store's
//! rules.

use std::cell::RefCell;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::MainThreadMarker;
use objc2_foundation::{NSError, NSString};
use objc2_web_kit::{WKContentRuleList, WKContentRuleListStore, WKWebViewConfiguration};

const RULE_LIST_IDENTIFIER: &str = "octoboard-no-web";

/// Every `http(s)` URL is blocked. A dev build loads its UI from the Vite dev server over plain
/// `http`, so there `localhost` is let through again; a release build has no such exception.
#[cfg(not(debug_assertions))]
const RULES: &str = r#"[{"trigger":{"url-filter":"^https?://"},"action":{"type":"block"}}]"#;
#[cfg(debug_assertions)]
const RULES: &str = r#"[
    {"trigger":{"url-filter":"^https?://"},"action":{"type":"block"}},
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
        eprintln!("octoboard: no WebKit content rule list store; report pages are not isolated.");
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
                eprintln!(
                    "octoboard: cannot compile the content rule list ({reason}); report pages \
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
