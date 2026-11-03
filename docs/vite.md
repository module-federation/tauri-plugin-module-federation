---
title: Vite (@module-federation/vite)
summary: Using @module-federation/tauri with Vite hosts and ES module remotes.
read_when:
  - adding a Vite host or remote
  - a Vite remote stays on "Loading" or fails CORS in a Tauri build
---

# Vite (`@module-federation/vite`)

Hosts built with [`@module-federation/vite`](https://github.com/module-federation/vite) use the same runtime plugin as Rsbuild and webpack hosts:

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

Hosts and remotes from different bundlers can be mixed. A Vite host can consume Rsbuild or webpack remotes (`type: 'var'`), and Vite remotes can consume Rsbuild remotes. The example apps do both.

## Requirements for Vite remotes

Vite remotes are ES modules, and the webview loads ES modules in CORS mode.

- **The remote server must send CORS headers** that allow the Tauri origin:

  | Build | Origin |
  |---|---|
  | Release on macOS and Linux | `tauri://localhost` |
  | Release on Windows | `http://tauri.localhost` |
  | `tauri dev` | your dev server URL |

  A remote that sends `Access-Control-Allow-Origin: *` works everywhere. Vite's default CORS policy only allows `localhost` origins, so set `preview.cors: true` (or your own allowlist) when you serve a remote with `vite preview`. Without these headers the remote never finishes loading.

- **Build the remote with a relative `base`** (`base: './'`). The remote's chunks then resolve relative to `remoteEntry.js` and go through `mf://`, which caches them for offline use. With an absolute `base` such as `https://cdn.example.com/`, chunks load straight from the network and fail offline.

The Tauri plugin forwards the webview's `Origin` header and caches response headers with each body, so cached ES module remotes pass the CORS and MIME checks offline too.

## Development notes

- **Run the dev server with `--force`.** `@module-federation/vite` pre-bundles runtime plugins with Vite's dependency optimizer. Vite does not notice when a workspace copy of `@module-federation/tauri` is rebuilt, so without `vite --force` the host keeps running the old plugin code. The example's `beforeDevCommand` is `pnpm vite --force`.
- **Disable type generation for JavaScript projects.** `@module-federation/vite` generates types by default and fails without a `tsconfig.json`. The examples set `dts: false`.
- **Prefetching.** `@module-federation/vite` can prefetch ES module remote entries over plain HTTP. Those requests bypass the Tauri plugin. The copy that actually runs is loaded through `mf://`.

See `example/vite-host` and `example/vite-guest` for a working setup.
