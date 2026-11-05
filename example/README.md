# Module Federation Example

Demonstrates chained Module Federation in a Tauri app with offline caching.

## Apps

| App | Bundler | Port | Role |
|---|---|---|---|
| `host` | Rsbuild | 3001 | Tauri + React app, consumes `guest` and `vite-guest` |
| `guest` | Rsbuild | 3002 | Standalone React app, exposes Button, consumes `guest-2` |
| `guest-2` | Rsbuild | 3003 | Standalone React app, exposes Button |
| `vite-guest` | Vite | 3004 | ES module remote, exposes Button, consumes `guest-2` |
| `vite-host` | Vite | 3005 | Tauri + React app, consumes `vite-guest` and `guest` |

Each app uses a different version of lodash to demonstrate Module Federation's multi-version shared dependency loading. React is shared across all apps.

## Development

```sh
pnpm dev
```

Turbo starts the guest dev servers with the host task. The host waits for the guest remotes to be ready before launching Tauri.

For the Vite host (`@module-federation/vite`):

```sh
pnpm dev:vite
```

The Vite host's dev server starts with `--force`. `@module-federation/vite` pre-bundles runtime plugins, and Vite does not notice when the workspace copy of `@module-federation/tauri` is rebuilt, so without `--force` the host keeps running the old plugin code.

`vite-guest` is built with `base: './'` so its chunks are cached for offline use, and `vite preview` sends CORS headers (`preview.cors: true`) so a release build (origin `tauri://localhost`) can load it.

More on Vite remotes: [docs/vite.md](../docs/vite.md).

## Production

```sh
pnpm build
```

## Zephyr Cloud

To enable [Zephyr Cloud](https://zephyr-cloud.io/) integration:

```sh
WITH_ZEPHYR=true pnpm dev
# or
WITH_ZEPHYR=true pnpm build
```

This wraps each Rsbuild config with `withZephyr()`, enabling Zephyr's module deployment and versioning.
