//! Smoke-test reporter for the example apps.
//!
//! When the `MF_E2E` environment variable is set, the app collects page errors,
//! waits for remote modules to finish loading (or `MF_E2E_WAIT_MS`), prints a single
//! `MF_E2E_REPORT {json}` line to stdout, and exits. Without `MF_E2E` this is a
//! no-op, so the examples behave like regular Tauri apps.

use std::time::Duration;

use tauri::{plugin::TauriPlugin, AppHandle, Manager, Runtime};

const REPORTER_SCRIPT: &str = r#"
(() => {
	if (window.top !== window) return;
	const errors = [];
	const describe = (value) => (value && value.stack ? `${value.message || value}\n${value.stack}` : String(value));
	const push = (kind, value) => errors.push(`${kind}: ${describe(value)}`);
	window.addEventListener('error', (e) => push('error', e.error || e.message));
	window.addEventListener('unhandledrejection', (e) => push('unhandledrejection', e.reason));
	const consoleError = console.error.bind(console);
	console.error = (...args) => { push('console.error', args.map(describe).join(' ')); consoleError(...args); };
	const warnings = [];
	const consoleWarn = console.warn.bind(console);
	console.warn = (...args) => { warnings.push(args.map(describe).join(' ')); consoleWarn(...args); };

	let reported = false;
	let lastText = null;
	let stableSince = Date.now();
	const report = (settled) => {
		if (reported) return;
		reported = true;
		const text = document.body ? document.body.innerText.trim() : '';
		window.__TAURI_INTERNALS__.invoke('e2e_report', {
			report: JSON.stringify({ settled, href: location.href, text, errors, warnings }),
		});
	};
	// Called by the Rust side once MF_E2E_WAIT_MS has passed. Timers in a background
	// window can be throttled, so the deadline is not left to setTimeout.
	window.__mfE2eReport = () => report(false);
	const tick = () => {
		if (reported) return;
		const text = document.body ? document.body.innerText.trim() : '';
		if (text !== lastText) { lastText = text; stableSince = Date.now(); }
		if (text.length > 0 && !/Loading/.test(text) && Date.now() - stableSince > 1500) return report(true);
		setTimeout(tick, 250);
	};
	setTimeout(tick, 250);
})();
"#;

#[tauri::command]
fn e2e_report<R: Runtime>(app: AppHandle<R>, report: String) {
    println!("MF_E2E_REPORT {report}");
    app.exit(0);
}

/// Returns the reporter plugin when `MF_E2E` is set. `MF_E2E_WAIT_MS` (default 20000)
/// caps how long the page may take to settle before it is reported as not settled.
pub fn plugin<R: Runtime>() -> Option<TauriPlugin<R>> {
    std::env::var_os("MF_E2E")?;
    let wait_ms: u64 = std::env::var("MF_E2E_WAIT_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(20_000);
    Some(
        tauri::plugin::Builder::new("example-e2e")
            .js_init_script(REPORTER_SCRIPT.to_string())
            .setup(move |app, _api| {
                let app = app.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(wait_ms));
                    for webview in app.webview_windows().values() {
                        let _ = webview.eval("window.__mfE2eReport && window.__mfE2eReport()");
                    }
                });
                Ok(())
            })
            .build(),
    )
}

/// App-level command handler for the reporter. App commands need no capability entry.
pub fn handler<R: Runtime>() -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![e2e_report]
}
