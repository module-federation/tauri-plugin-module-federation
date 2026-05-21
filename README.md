# Tauri Module Federation

Use Module Federation in Tauri apps with `@module-federation/tauri` and the companion Rust plugin `tauri-plugin-module-federation`.

> [!IMPORTANT]
> `@module-federation/tauri` rewrites remote entries to the `module-federation://` protocol. Your Tauri app must register `tauri-plugin-module-federation` so those requests can be fetched, cached, and replayed offline.

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
    .plugin(tauri_plugin_module_federation::init())
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
```

## Example apps

- Host app: `example/host` on `http://localhost:3001`
- Guest app: `example/guest` on `http://localhost:3002`
- Guest app 2: `example/guest-2` on `http://localhost:3003`
- Start all apps from repo root with `pnpm --dir example dev`
- More example details: `example/README.md`

## Behavior

- Remote entry URLs are rewritten from `http(s)://...` to `module-federation://.../?fullUrl=...`.
- The Tauri plugin fetches those assets over the network and stores them in the app cache dir.
- Cache keys are derived from the remote host and request path.
- If a later fetch fails, the cached asset is served instead.

## Build checks

```bash
pnpm install
pnpm --filter example_guest build
pnpm --filter 'example-guest_2' build
pnpm --filter example-host exec rsbuild build
```

## Repo layout

- Rust plugin: `tauri-plugin`
- Runtime package: `module-federation-plugin`
- Example apps: `example`

## License

MIT
