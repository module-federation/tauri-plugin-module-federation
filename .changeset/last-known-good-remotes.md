---
'@module-federation/tauri': minor
---

Keep remotes working through outages and broken releases.

- `tauri-plugin-module-federation` only caches successful responses. When a remote is offline, times out, or answers with an error status, the last cached copy is served, so a broken deploy can no longer overwrite a working copy. Requests have connect and read timeouts, and a remote that stopped responding is served from the cache for 30 seconds without waiting for each request to time out.
- A new version of a remote entry that fails to load is rolled back to the last version that loaded successfully: the page reloads with it, and later launches skip the failed version for 24 hours or until the remote publishes another one. Enabled by default in release builds.
- New `tauri_plugin_module_federation::Builder` to configure the cache folder, timeouts and rollback. `init()` keeps working with the defaults.
- Requests that cannot be answered return an HTTP error instead of panicking the protocol handler.
- `@module-federation/tauri` URL-encodes the remote URL (query strings were cut off before) and keeps the entry's path in the `mf://` URL, so remotes hosted under a sub-path resolve their chunks correctly.
