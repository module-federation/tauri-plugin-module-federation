# Tauri Module Federation

Use Module Federation in Tauri apps with `@module-federation/tauri` and the companion Rust plugin `tauri-plugin-module-federation`.

> [!IMPORTANT]
> `@module-federation/tauri` rewrites remote entries to the `mf://` protocol. Your Tauri app must register `tauri-plugin-module-federation` so those requests can be fetched, cached, and replayed offline.

## What you get

- A Module Federation runtime plugin for Tauri hosts, for Rsbuild, webpack and Vite (`@module-federation/vite`).
- A Tauri custom protocol handler that fetches remote files and caches them on disk.
- Remotes that keep working offline, after an app restart, and when a remote answers with errors or hangs.
- Automatic rollback to the last version that loaded when a new release of a remote fails to load.

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

Vite remotes must send CORS headers that allow the Tauri origin, and should be built with `base: './'` so their chunks are cached. See [docs/vite.md](./docs/vite.md).

Hosts and remotes from different bundlers can be mixed: both example hosts consume an Rsbuild remote and a Vite remote. Rsbuild and webpack hosts register ES module remotes at runtime instead of in their config. See [Rsbuild or webpack host with a Vite remote](./docs/vite.md#rsbuild-or-webpack-host-with-a-vite-remote).

## Example apps

| App | Bundler | Port | Role |
|---|---|---|---|
| `example/host` | Rsbuild | 3001 | Tauri host, consumes `guest` and `vite-guest` |
| `example/guest` | Rsbuild | 3002 | Remote, consumes `guest-2` |
| `example/guest-2` | Rsbuild | 3003 | Remote |
| `example/vite-guest` | Vite | 3004 | ES module remote, consumes `guest-2` |
| `example/vite-host` | Vite | 3005 | Tauri host, consumes `vite-guest` and `guest` |

- Start the Rsbuild host and its remotes with `pnpm dev`.
- Start the Vite host and its remotes with `pnpm dev:vite`.
- More example details: `example/README.md`

## Offline and failed releases

Every remote file loaded through `mf://` is cached in the app cache dir, so the cache survives app restarts.

| Situation | What the app gets |
|---|---|
| Remote reachable, responds 2xx | The fresh response. It replaces the cached copy. |
| Offline, or the remote answers an error status | The cached copy. Error responses are never cached. |
| Remote accepts the connection but does not answer | The cached copy after the read timeout (10 s by default). |
| Not reachable and never cached | A `502`/`504` error that the Module Federation runtime reports as a failed remote. |
| A new release that fails to load | The page reloads with the last version that loaded. That version is kept on later launches. |

Rollback is on by default in release builds and off in debug builds. Configure it, the timeouts and the cache folder with `tauri_plugin_module_federation::Builder`:

```rust
tauri::Builder::default()
    .plugin(
        tauri_plugin_module_federation::Builder::new()
            .cache_folder("module-federation")
            .connect_timeout(std::time::Duration::from_secs(10))
            .read_timeout(std::time::Duration::from_secs(10))
            .rollback(true)
            .build(),
    )
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
```

How it works, the cache layout and the known limitations are in [docs/offline-and-rollback.md](./docs/offline-and-rollback.md).

## Documentation

- [Architecture](./docs/architecture.md): how `mf://` requests flow through both packages.
- [Offline and failed releases](./docs/offline-and-rollback.md): caching, timeouts, last-known-good rollback, configuration, limitations.
- [Vite](./docs/vite.md): `@module-federation/vite` hosts and ES module remotes.
- [Testing](./docs/testing.md): unit tests, build checks and the end-to-end smoke test.
- [Releasing](./docs/RELEASING.md): publishing the npm package and the Cargo crate.

## Build checks

```bash
pnpm install
pnpm build
```

## Repo layout

- Rust plugin: `packages/tauri-plugin`
- Runtime package: `packages/tauri`
- Example apps: `example/*`

## License

[MIT](./LICENSE), the same license as [Module Federation core](https://github.com/module-federation/core).
