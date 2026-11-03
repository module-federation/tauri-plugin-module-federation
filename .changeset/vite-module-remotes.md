---
'@module-federation/tauri': patch
---

Support `@module-federation/vite` hosts and ES module remotes. `tauri-plugin-module-federation` now forwards the webview's `Origin` header and caches response headers with each body, so ES module remotes pass the webview's CORS and MIME checks both online and when served from the offline cache.
