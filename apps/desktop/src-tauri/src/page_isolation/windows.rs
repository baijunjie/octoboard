//! Page isolation on Windows: the window's WebView2 answers `http(s)` requests other than image
//! loads with a 403 (`policy::is_blocked` decides), for the reasons `macos.rs` gives for its content
//! rule list. The filter sees requests only: a `<link rel="preconnect">` was measured opening
//! its connection to an address the filter refuses, with no request sent on it, so resource
//! hints are closed by the sanitizer alone. This module serves every target but macOS, Linux
//! included, where there is no WebView2 and nothing is installed.
//!
//! WebView2 has no setting that is in place before the window exists, so the filter is installed on
//! the window once it is built (`install`). The page's first request, the app's own `index.html`, is
//! one the filter lets through, and the report frame loads only on the user's action.

use crate::MainWindowBuilder;

/// Nothing is prepared for the window's builder.
pub struct Configuration;

/// Hands `then` its configuration at once: nothing has to be ready before the window is built.
pub fn prepare(then: impl FnOnce(Configuration) + 'static) {
    then(Configuration);
}

/// Leaves the builder as it is.
pub fn apply(
    builder: MainWindowBuilder<'_>,
    _configuration: Configuration,
) -> MainWindowBuilder<'_> {
    builder
}

/// Installs the request filter on `window`'s webview. A failure is logged and the window stays as it
/// is: the sanitizer is the primary barrier, and a window that does not open is worse.
#[cfg(windows)]
pub fn install(window: &tauri::WebviewWindow) {
    use crate::diagnostics::diagnostic;

    // Runs at once when called on the main thread, which is where the window is built.
    let queued = window.with_webview(|webview| {
        if let Err(err) = filter_web_requests(&webview.controller(), &webview.environment()) {
            diagnostic!("cannot install the request filter; report pages rely on the sanitizer alone: {err}");
        }
    });
    if let Err(err) = queued {
        diagnostic!(
            "cannot reach the window's webview; report pages rely on the sanitizer alone: {err}"
        );
    }
}

#[cfg(not(windows))]
pub fn install(_window: &tauri::WebviewWindow) {}

#[cfg(windows)]
fn filter_web_requests(
    controller: &webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Controller,
    environment: &webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Environment,
) -> windows::core::Result<()> {
    use webview2_com::Microsoft::Web::WebView2::Win32::*;
    use webview2_com::{take_pwstr, WebResourceRequestedEventHandler};
    use windows::core::{Interface, HSTRING, PWSTR};

    use super::policy::is_blocked;

    // SAFETY: COM calls on the main thread, which owns the webview, with arguments that outlive them.
    unsafe {
        let webview = controller.CoreWebView2()?;
        // Wry registers the app's own protocols with `ALL` request source kinds where WebView2 has
        // the newer call, so iframes (the report frame) and workers are covered; the plain call
        // covers only the main document's requests.
        let sources = webview.cast::<ICoreWebView2_22>().ok();
        for pattern in ["http://*", "https://*"] {
            let pattern = HSTRING::from(pattern);
            match &sources {
                Some(webview) => webview.AddWebResourceRequestedFilterWithRequestSourceKinds(
                    &pattern,
                    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                    COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL,
                )?,
                None => webview.AddWebResourceRequestedFilter(
                    &pattern,
                    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                )?,
            }
        }

        let environment = environment.clone();
        let handler = WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else {
                return Ok(());
            };
            let mut uri = PWSTR::null();
            args.Request()?.Uri(&mut uri)?;
            let uri = take_pwstr(uri);
            let mut context = COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL;
            args.ResourceContext(&mut context)?;
            // Wry's own handler, registered first and for the app's hosts only, answers those; a
            // request this one lets through is left for it or for the network.
            if is_blocked(
                &uri,
                context == COREWEBVIEW2_WEB_RESOURCE_CONTEXT_IMAGE,
                cfg!(debug_assertions),
            ) {
                let response = environment.CreateWebResourceResponse(
                    None,
                    403,
                    &HSTRING::from("Forbidden"),
                    &HSTRING::new(),
                )?;
                args.SetResponse(&response)?;
            }
            Ok(())
        }));
        // The filter lives as long as the webview, so the registration is never removed.
        let mut token = 0i64;
        webview.add_WebResourceRequested(&handler, &mut token)
    }
}
