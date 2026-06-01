# Module Federation Example

Demonstrates chained Module Federation in a Tauri app with offline caching.

## Apps

| App | Port | Role |
|---|---|---|
| `host` | 3001 | Tauri + React app, consumes `guest` |
| `guest` | 3002 | Standalone React app, exposes Button, consumes `guest-2` |
| `guest-2` | 3003 | Standalone React app, exposes Button |

Each app uses a different version of lodash to demonstrate Module Federation's multi-version shared dependency loading. React is shared across all apps.

## Development

```sh
pnpm dev
```

Turbo starts the guest dev servers with the host task. The host waits for the guest remotes to be ready before launching Tauri.

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

This wraps each rspack config with `withZephyr()`, enabling Zephyr's module deployment and versioning.
