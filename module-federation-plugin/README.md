# `@module-federation/tauri`

Use Module Federation in Tauri hosts with `@module-federation/tauri`.

> [!IMPORTANT]
> This package only rewrites remote entry URLs. Your Tauri app must also install and register `tauri-plugin-module-federation`.

## What you get

- A Module Federation runtime plugin for Tauri hosts.
- URL rewriting from `http(s)://...` to `module-federation://...`.
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

## Behavior

- Hooks `afterResolve` and rewrites `remoteInfo.entry`.
- Registers a global runtime plugin during `beforeInit`.
- Delegates fetching/caching to `tauri-plugin-module-federation`.

## License

MIT
