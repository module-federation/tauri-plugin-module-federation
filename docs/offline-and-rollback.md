---
title: Offline and failed releases
summary: How remotes are cached, served offline, and rolled back to the last version that loaded.
read_when:
  - changing caching, timeouts or rollback in packages/tauri-plugin
  - a remote shows an old version or does not update
  - explaining what happens when a remote is down or a release is broken
---

# Offline and failed releases

Every file loaded through `mf://` is fetched by the Tauri plugin and cached under the app cache dir (`<app cache dir>/module-federation/` by default), so the cache survives app restarts.

## What the app gets

| Situation | Response |
|---|---|
| Remote reachable, answers 2xx | The fresh response. It replaces the cached copy. |
| Offline, DNS failure, connection refused | The cached copy. |
| Remote answers an error status (such as a broken deploy returning 500 or 404) | The cached copy. Error responses are never cached, so they cannot overwrite a working copy. |
| Remote accepts the connection but does not answer | The cached copy after the read timeout (10 s by default). For the next 30 s, other requests to that origin use the cache without waiting. |
| Not reachable and never cached (such as the first launch offline) | `502`, or `504` on a timeout. The Module Federation runtime reports a failed remote. |
| A new release that fails to load (missing chunks, an entry that throws) | The page reloads with the last version that loaded. See [Last-known-good entries](#last-known-good-entries). |

## Network and cache rules

- **Network first.** Every request goes to the network. The cache is used only when the network fails, times out, or answers with a non-2xx status.
- **Only 2xx responses are cached**, and only for `GET` requests.
- **Headers are cached with the body** and replayed, so ES module remotes still pass the webview's CORS and MIME checks offline. The hop-by-hop headers `connection`, `keep-alive` and `transfer-encoding` are dropped.
- **Query strings are ignored** in cache keys, so cache-busting parameters such as `?t=<timestamp>` still hit the cache offline.
- **Timeouts.** Connecting may take up to 10 s. Reading has a 10 s inactivity timeout, not a total limit, so large downloads on slow networks are not cut off.
- **Circuit breaker.** After a network failure, requests to that origin that have a cached copy are answered from the cache for 30 s without trying the network. Requests without a cached copy still try the network.
- **Origin is forwarded.** The webview's `Origin` header is sent upstream, so servers with an origin allowlist (such as the Vite dev server) answer with CORS headers.
- **Body errors are network errors.** A response whose body fails halfway is treated as a failure, not cached as a success.

Responses served from the cache carry an `x-mf-cache` header: `stale` for a cached copy standing in for the network, and `last-known-good` for an entry served instead of a rejected version.

## Last-known-good entries

Caching alone does not help when a remote publishes a release that loads but is broken: the network answers 2xx, so the broken version replaces the cached copy. The Tauri plugin therefore tracks versions of each remote **entry** (`remoteEntry.js`) by content hash.

```text
              loads (onLoad → mf-commit)
   trial ─────────────────────────────────▶ last-known-good
     │
     │ a remote fails (errorLoadRemote → mf-rollback)
     ▼
  rejected ── skipped for 24 h, or until the remote publishes another version
```

1. **Commit.** When a remote loads, the runtime plugin's `onLoad` hook sends `mf-commit`. The Tauri plugin copies the version it served to the last-known-good slot.
2. **Trial.** When the network serves an entry that differs from the last-known-good copy, it is served as a trial for this launch.
3. **Rollback.** When a remote fails to load, the runtime plugin's `errorLoadRemote` hook sends `mf-rollback`. Every trial not yet committed during this launch is rejected, and the page reloads. The reload also resets shared modules the broken release may have registered, which an in-page retry cannot undo.
4. **Skip.** On the reload, and on later launches, a rejected version is replaced by the last-known-good copy. After 24 hours it gets another trial. A different new version gets a trial right away.

Limits and defaults:

- At most 3 rollbacks per launch, enforced in Rust. A remote that keeps publishing broken versions cannot cause endless reloads.
- Rollback is on by default in release builds and off in debug builds, so a broken remote under `tauri dev` shows its error instead of an older version. Offline fallback is always on.
- Entries are served with `Cache-Control: no-store`, because the version served is decided per launch. Without it, the webview's own cache would bring a rejected version back on reload. Chunks keep their upstream caching headers.
- Chunks can import their entry by its plain URL (Vite remotes do). Those requests get the same version as the entry, not whatever the network has now.

Rollback relies on content-hashed chunk names, the default in production builds of Rsbuild, webpack and Vite. An older entry still requests its own chunks, which must exist on the server or in the cache.

## Configuration

```rust
use std::time::Duration;

tauri::Builder::default()
    .plugin(
        tauri_plugin_module_federation::Builder::new()
            .cache_folder("module-federation")      // folder inside the app cache dir
            .connect_timeout(Duration::from_secs(10))
            .read_timeout(Duration::from_secs(10))  // inactivity timeout, not a total limit
            .rollback(true)                         // default: release builds only
            .build(),
    )
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
```

`tauri_plugin_module_federation::init(None)` uses the defaults, and `init(Some("folder"))` only changes the cache folder.

These values are fixed:

| Constant | Value |
|---|---|
| Circuit breaker window | 30 s |
| Rejected version skipped for | 24 h |
| Rollbacks per launch | 3 |

## Cache layout

Files are stored under the cache folder, grouped by remote origin:

```text
<sha1(scheme://host[:port])>/<sha1(path)>               body of the last 2xx response
<sha1(scheme://host[:port])>/<sha1(path)>.headers       its headers, one `name: value` per line
<sha1(scheme://host[:port])>/<sha1(path)>.good          last-known-good copy (entries only)
<sha1(scheme://host[:port])>/<sha1(path)>.good.headers
<sha1(scheme://host[:port])>/<sha1(path)>.rejected      hash of a version that failed to load
```

- Writes go to a temporary file first and are then renamed into place, so a crash never leaves a half-written file.
- Entries cached by older versions of the plugin have no `.headers` file. They are served without headers.
- To reset everything, quit the app and delete the cache folder. Rejection times come from the `.rejected` file's modification time.

## Debugging

- **Inspect responses.** Open the webview devtools and look for `x-mf-cache` on `mf://` responses.
- **Read the logs.** The Tauri plugin logs fallbacks, rollbacks and cache write failures through the [`log`](https://docs.rs/log) crate. Add a logger such as `tauri-plugin-log` to see them.
- **Console warnings.** The runtime plugin warns in the console when it rolls back and reloads, and when a commit or rollback request fails.

## Limitations

- **Content-hashed chunks are required for rollback.** A last-known-good entry loads its own chunks, so they must not be overwritten in place by a newer release.
- **A failure is blamed on every unconfirmed trial.** If remote A fails while remote B has a new version that loaded but was not used yet, B's version is rejected too. It gets another trial after 24 hours, or as soon as B publishes another version.
- **Only load errors trigger a rollback.** A release that loads but then crashes the webview, renders nothing, or hangs without throwing is not detected.
- **Manifest remotes are not covered.** `mf-manifest.json` remotes are fetched by the Module Federation runtime directly, not through `mf://`, so they get no offline cache and no rollback.
- **Vite hosts may also fetch entries directly.** `@module-federation/vite` can prefetch ES module remote entries over plain HTTP. The copy that actually runs still comes through `mf://`.
- **Tested on macOS.** The Windows form, `http://mf.localhost/...`, has not been tested end to end.
