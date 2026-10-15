// Opens the UI in the system WKWebView (the engine the desktop app runs in), injects the instrument
// script at document start and the driver script once the page has loaded, prints every JSON line the
// driver posts to `webkit.messageHandlers.bench`, and exits on the line that says `"done":true`.
//
//   host <url> <instrument.js> <driver.js> [timeoutSeconds=240]
//
// The timeout ends the run with an error line; the caller sets it to what the scenario may take.
//
// The window is ordered front and the app activated: WebKit runs no frames for a page nobody can see,
// so a hidden or background window measures a frozen page. The web data store is non-persistent, so a
// run neither reads nor leaves anything in the user's own WebKit profile.
import Cocoa
import WebKit

let args = CommandLine.arguments
guard args.count >= 4, let url = URL(string: args[1]) else {
  FileHandle.standardError.write(Data("usage: host <url> <instrument.js> <driver.js> [timeoutSeconds]\n".utf8))
  exit(2)
}
let instrument = try! String(contentsOfFile: args[2], encoding: .utf8)
let driver = try! String(contentsOfFile: args[3], encoding: .utf8)
let timeout = args.count > 4 ? Double(args[4]) ?? 240 : 240

func emit(_ line: String) {
  print(line)
  fflush(stdout)
}

func emitError(_ message: String) {
  let data = try! JSONSerialization.data(withJSONObject: ["error": message])
  emit(String(data: data, encoding: .utf8)!)
}

class Handler: NSObject, WKScriptMessageHandler, WKNavigationDelegate {
  func userContentController(_ c: WKUserContentController, didReceive m: WKScriptMessage) {
    let line = m.body as? String ?? "\(m.body)"
    emit(line)
    if line.contains("\"done\":true") { NSApp.terminate(nil) }
  }

  func webView(_ w: WKWebView, didFinish n: WKNavigation!) {
    // The driver is an async function whose promise WebKit cannot hand back; ending on a number keeps
    // that out of the completion's error.
    w.evaluateJavaScript(driver + "\n;0") { _, e in if let e = e { emitError("\(e)") } }
  }
}

let app = NSApplication.shared
app.setActivationPolicy(.regular)
let handler = Handler()
let config = WKWebViewConfiguration()
config.websiteDataStore = .nonPersistent()
config.userContentController.addUserScript(
  WKUserScript(source: instrument, injectionTime: .atDocumentStart, forMainFrameOnly: true))
config.userContentController.add(handler, name: "bench")
let window = NSWindow(
  contentRect: NSRect(x: 60, y: 60, width: 1440, height: 900), styleMask: [.titled, .resizable],
  backing: .buffered, defer: false)
let web = WKWebView(frame: window.contentView!.bounds, configuration: config)
web.autoresizingMask = [.width, .height]
web.navigationDelegate = handler
window.contentView!.addSubview(web)
window.title = "octoboard wkbench"
window.orderFrontRegardless()
app.activate(ignoringOtherApps: true)
// A regular app is napped (its timers and frames throttled) once it is hidden or idle, and the display going
// to sleep hides its window: either stalls the page mid-run, so both are held off for as long as it runs.
let activity = ProcessInfo.processInfo.beginActivity(
  options: [.userInitiated, .idleDisplaySleepDisabled], reason: "wkbench measures a live page")
var warnedOccluded = false
let watch = Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { _ in
  if window.occlusionState.contains(.visible) { return }
  if !warnedOccluded {
    warnedOccluded = true
    FileHandle.standardError.write(Data("wkbench: the window is not visible; WebKit runs no frames for it\n".utf8))
  }
  window.orderFrontRegardless()
  app.activate(ignoringOtherApps: true)
}
web.load(URLRequest(url: url))
DispatchQueue.main.asyncAfter(deadline: .now() + timeout) {
  emitError("timeout after \(Int(timeout)) s")
  NSApp.terminate(nil)
}
app.run()
