# Tauri Module Federation

Use Module Federation in Tauri apps with `@module-federation/tauri` and the companion Rust plugin `tauri-plugin-module-federation`.

> [!IMPORTANT]
> `@module-federation/tauri` rewrites remote entries to the `mf://` protocol. Your Tauri app must register `tauri-plugin-module-federation` so those requests can be fetched, cached, and replayed offline.

## What you get

- A Module Federation runtime plugin for Tauri hosts.
- A Tauri custom protocol handler that fetches and caches remote assets.
- Offline fallback when a remote module has already been fetched once.

## Package usage

```ts
import { pluginModuleFederation } from '@module-federation/rsbuild-plugin';

export default {
	plugins: [
		pluginModuleFederation({
			name: 'host',
			remotes: {
				remote: 'remote@http://localhost:3002/remoteEntry.js',
			},
			runtimePlugins: ['@module-federation/tauri'],
		}),
	],
};
```

```rust
tauri::Builder::default()
    // Defaults. Use tauri_plugin_module_federation::Builder to configure the plugin.
    .plugin(tauri_plugin_module_federation::init(None))
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
```

### Vite (`@module-federation/vite`)

Hosts built with [`@module-federation/vite`](https://github.com/module-federation/vite) use the same runtime plugin:

```ts
import { federation } from '@module-federation/vite';

export default {
	plugins: [
		federation({
			name: 'host',
			remotes: {
				remote: {
					type: 'module',
					name: 'remote',
					entry: 'http://localhost:3004/remoteEntry.js',
				},
			},
			runtimePlugins: ['@module-federation/tauri'],
		}),
	],
};
```

Vite remotes are ES modules, and the webview loads ES modules in CORS mode. Two rules apply to Vite remotes:

- **The remote server must send CORS headers** that allow the Tauri origin: `tauri://localhost` in release builds on macOS and Linux, `http://tauri.localhost` on Windows, and your dev server URL under `tauri dev`. A remote that sends `Access-Control-Allow-Origin: *` works everywhere. Vite's default CORS policy only allows `localhost` origins, so set `preview.cors: true` (or your own allowlist) when you serve a remote with `vite preview`. Without these headers the remote never finishes loading.
- **Build the remote with a relative `base`** (`base: './'`). The remote's chunks then resolve relative to `remoteEntry.js` and go through `mf://`, which caches them for offline use. With an absolute `base` such as `https://cdn.example.com/`, chunks load straight from the network and fail offline.

Hosts and remotes from different bundlers can be mixed. A Vite host can consume rsbuild/webpack (`type: 'var'`) remotes, and Vite remotes can consume rsbuild remotes.

## Example apps

| App | Bundler | Port | Role |
|---|---|---|---|
| `example/host` | Rsbuild | 3001 | Tauri host, consumes `guest` |
| `example/guest` | Rsbuild | 3002 | Remote, consumes `guest-2` |
| `example/guest-2` | Rsbuild | 3003 | Remote |
| `example/vite-guest` | Vite | 3004 | ES module remote, consumes `guest-2` |
| `example/vite-host` | Vite | 3005 | Tauri host, consumes `vite-guest` and `guest` |

- Start the Rsbuild host and its remotes with `pnpm dev`.
- Start the Vite host and its remotes with `pnpm dev:vite`.
- More example details: `example/README.md`

## Offline and failed releases

Every remote file loaded through `mf://` is fetched by the Tauri plugin and cached in the app cache dir (`<app cache dir>/module-federation/`), so the cache survives app restarts.

| Situation | What the app gets |
|---|---|
| Remote reachable, responds 2xx | The fresh response. It replaces the cached copy. |
| Offline, DNS failure, connection refused | The cached copy. |
| Remote answers an error status (e.g. a broken deploy returning 500 or 404) | The cached copy. Error responses are never cached, so they cannot overwrite a working copy. |
| Remote accepts the connection but does not answer | The cached copy after the read timeout (10 s by default). For the next 30 s, other requests to that remote use the cache without waiting. |
| Not reachable and never cached (e.g. first launch offline) | A `502`/`504` error that the Module Federation runtime reports as a failed remote. |
| A new release that fails to load (missing chunks, an entry that throws) | The page reloads with the last version that loaded successfully. See below. |

### Last-known-good releases

When a remote loads successfully, the runtime plugin tells the Tauri plugin, which keeps that version of the remote's entry as *last-known-good*. When the remote later publishes a different entry, it gets a trial in the current launch:

- If it loads, it becomes the new last-known-good version.
- If any remote fails to load while a trial is unconfirmed, the trial versions are rejected and the page reloads with the last-known-good entries. The reload also resets shared modules a broken release may have registered. Rejected versions are skipped on later launches too, for 24 hours or until the remote publishes another version.

This relies on content-hashed chunk file names (the default for production builds of Rsbuild, webpack and Vite): an older entry still finds its own chunks, from the network or the cache. Rollbacks are capped at 3 per launch. Rollback is enabled by default in release builds and disabled in debug builds, so that during development a broken remote shows its error.

### Configuration

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

`tauri_plugin_module_federation::init(None)` uses the defaults.

### Behavior details

- Remote entry URLs are rewritten from `http(s)://host/path/remoteEntry.js` to `mf://host/path/remoteEntry.js?fullUrl=<encoded URL>`. The path is kept so relative chunk URLs resolve under it.
- The webview's `Origin` header is forwarded, so servers with an origin allowlist answer with CORS headers.
- Cache keys are derived from the remote origin and request path; query strings are ignored, so cache-busting parameters still hit the cache. Response headers are cached with the body.
- Responses served from the cache carry an `x-mf-cache: stale` or `x-mf-cache: last-known-good` header.
- Remote entries are served with `Cache-Control: no-store`, because the plugin decides per launch which version to serve.
- Manifest remotes (`mf-manifest.json`) are fetched by the Module Federation runtime directly, not through `mf://`, and are not cached.

### Limitations

- **Content-hashed chunks are required for rollback.** A last-known-good entry loads its own chunks, so they must not be overwritten in place by a newer release. Production Rsbuild, webpack and Vite builds hash chunk names by default.
- **A failure is blamed on every unconfirmed trial.** If remote A fails while remote B has a new version that loaded but was not used yet, B's version is rejected too. It gets another trial after 24 hours, or as soon as B publishes another version.
- **Only load errors trigger a rollback.** A release that loads but then crashes the webview, renders nothing, or hangs without throwing is not detected.
- **Manifest remotes are not covered.** `mf-manifest.json` remotes skip `mf://`, so they get no offline cache and no rollback.
- **Vite hosts may also fetch entries directly.** `@module-federation/vite` can prefetch ESM remote entries over plain HTTP. The copy that actually runs still comes through `mf://`.
- **Tested on macOS.** On Windows, Tauri serves custom schemes as `http://mf.localhost/...`. That path has not been tested end to end.

## Build checks

```bash
pnpm install
pnpm build
```

## Smoke test

`scripts/e2e-smoke.mjs` runs a built example app against locally served remotes. Each scenario starts with an empty cache and launches the app several times, like closing and reopening it, with the remote servers in a different state for each launch:

| Scenario | Launches |
|---|---|
| `restart` | healthy, then offline |
| `outage` | healthy, then every request answers HTTP 500, then offline |
| `broken-release` | healthy, then a new `remoteEntry.js` whose chunks are missing, then offline |
| `broken-entry` | healthy, then a new `remoteEntry.js` that throws, then offline |
| `hang` | healthy, then servers that never answer, then offline |
| `cold-offline` | offline on the first launch: the app must start and report cleanly |

Each launch must render every `--expect` text. Most must also log no page errors. The example apps only enable the reporter when `MF_E2E` is set.

```bash
# Build the remotes, then the hosts with `pnpm tauri build --bundles app` in example/host and example/vite-host.
pnpm e2e:smoke --app target/release/app \
  --cache "~/Library/Caches/dev.crabnebula.module-federation/module-federation" \
  --remote example/guest/dist:3002 --remote example/guest-2/dist:3003 \
  --expect "Host Button" --expect "Guest Button" --expect "Guest 2 Button"

pnpm e2e:smoke --app target/release/vite-host-app \
  --cache "~/Library/Caches/dev.module-federation.vite-host/module-federation" \
  --remote example/vite-guest/dist:3004 --remote example/guest/dist:3002 --remote example/guest-2/dist:3003 \
  --expect "Vite Guest Button" --expect "Guest Button" --expect "Guest 2 Button"
```

Pass `--scenario <name>` to run one scenario, and `--verbose` to print page warnings and the requests each remote received. The `--cache` path is the app cache dir for the app's identifier; the ones above are the macOS locations.

## Repo layout

- Rust plugin: `packages/tauri-plugin`
- Runtime package: `packages/tauri`
- Example apps: `example/*`

## License

MIT
