# tauri-plugin-module-federation

Tauri 2 plugin that loads [Module Federation](https://module-federation.io) remotes through an `mf://` custom protocol, caches every remote file on disk, and keeps remotes working offline.

> [!IMPORTANT]
> This crate is the Rust half of the integration. The host's frontend must also use the [`@module-federation/tauri`](https://www.npmjs.com/package/@module-federation/tauri) runtime plugin, which rewrites remote entries from `http(s)://` to `mf://` so their requests reach this plugin.

## What you get

- An `mf://` protocol handler that fetches remote files and caches them in the app cache dir, so the cache survives restarts.
- Remotes that keep loading offline, when a remote answers with an error status, or when it accepts the connection but never answers.
- Automatic rollback to the last version that loaded when a new release of a remote fails to load.

## Install

```bash
cargo add tauri-plugin-module-federation
npm install @module-federation/tauri
```

## Usage

Register the plugin with the defaults:

```rust
tauri::Builder::default()
    .plugin(tauri_plugin_module_federation::init(None))
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
```

Then add the runtime plugin to the host's Module Federation config, for example with Rsbuild:

```ts
pluginModuleFederation({
	name: 'host',
	remotes: {
		remote: 'remote@http://localhost:3002/remoteEntry.js',
	},
	runtimePlugins: ['@module-federation/tauri'],
});
```

Webpack and `@module-federation/vite` hosts use the same `runtimePlugins` entry. See the [Vite guide](https://github.com/module-federation/tauri/blob/main/docs/vite.md) for Vite remotes.

## Configuration

Use `Builder` to set the cache folder, the timeouts, and rollback:

```rust
use std::time::Duration;

tauri::Builder::default()
    .plugin(
        tauri_plugin_module_federation::Builder::new()
            .cache_folder("module-federation")
            .connect_timeout(Duration::from_secs(10))
            .read_timeout(Duration::from_secs(10))
            .rollback(true)
            .build(),
    )
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
```

Rollback is on by default in release builds and off in debug builds.

## Offline behavior

| Situation | What the app gets |
|---|---|
| Remote reachable, responds 2xx | The fresh response. It replaces the cached copy. |
| Offline, or the remote answers an error status | The cached copy. Error responses are never cached. |
| Remote accepts the connection but does not answer | The cached copy after the read timeout (10 s by default). |
| Not reachable and never cached | A `502`/`504` error that the Module Federation runtime reports as a failed remote. |
| A new release that fails to load | The page reloads with the last version that loaded. That version is kept on later launches. |

The cache layout and known limitations are in [Offline and failed releases](https://github.com/module-federation/tauri/blob/main/docs/offline-and-rollback.md). How requests flow through both packages is in [Architecture](https://github.com/module-federation/tauri/blob/main/docs/architecture.md).

## License

MIT
