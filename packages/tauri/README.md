# `@module-federation/tauri`

Use Module Federation in Tauri hosts with `@module-federation/tauri`.

> [!IMPORTANT]
> This package only rewrites remote entry URLs. Your Tauri app must also install and register `tauri-plugin-module-federation`.

## What you get

- A Module Federation runtime plugin for Tauri hosts.
- URL rewriting from `http(s)://...` to `mf://...`.
- Compatibility with the Rust-side cache and offline fallback flow.

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

With `@module-federation/vite`, add the same entry to `runtimePlugins`:

```ts
import { federation } from '@module-federation/vite';

federation({
	name: 'host',
	remotes: {
		remote: { type: 'module', name: 'remote', entry: 'http://localhost:3004/remoteEntry.js' },
	},
	runtimePlugins: ['@module-federation/tauri'],
});
```

Vite remotes must send CORS headers that allow the Tauri origin and should be built with `base: './'`. See [docs/vite.md](https://github.com/module-federation/tauri/blob/main/docs/vite.md) for details.

## Behavior

- `afterResolve`: rewrites http(s) `remoteInfo.entry` URLs to `mf://`, keeping the path.
- `onLoad`: tells `tauri-plugin-module-federation` that the remote's entry loaded, so it keeps that version as last-known-good.
- `errorLoadRemote`: when a remote fails to load, asks the Tauri plugin to roll back new, unconfirmed entry versions. If any were rolled back, the page reloads with the last versions that worked.
- Registers the same hooks as a global runtime plugin during `beforeInit`, so nested remotes are covered too.

Fetching, caching, offline fallback and rollback happen in `tauri-plugin-module-federation`. See [docs/offline-and-rollback.md](https://github.com/module-federation/tauri/blob/main/docs/offline-and-rollback.md) and [docs/architecture.md](https://github.com/module-federation/tauri/blob/main/docs/architecture.md).

## License

MIT
