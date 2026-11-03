---
title: Testing
summary: Unit tests, build checks, and the end-to-end smoke test for outages and broken releases.
read_when:
  - changing packages/tauri or packages/tauri-plugin
  - running or extending scripts/e2e-smoke.mjs
---

# Testing

## Checks

```bash
pnpm install
pnpm build                                   # npm package, example apps (full `tauri build`), cargo package
cargo test -p tauri-plugin-module-federation  # unit tests and doctest
cargo clippy --workspace -- -D warnings
cargo fmt --check
pnpm release:sync:check
```

The unit tests in `packages/tauri-plugin` cover:

- **`cache.rs`:** header round trips, origins kept apart, query strings ignored, commit and rejection rules, and atomic writes.
- **`protocol.rs`:** URL resolution, plus a local HTTP server that plays five launches of a remote releasing a broken version and checks the rollback. They also cover disabled rollback, the per-launch rollback limit, and the gateway error for a remote that was never cached.

## Smoke test

`scripts/e2e-smoke.mjs` runs a built example app against locally served remotes. Each scenario starts with an empty cache and launches the app several times, a fresh process each time, like closing and reopening it. The remote servers are in a different state for each launch:

| Scenario | Launches |
|---|---|
| `restart` | healthy, then offline |
| `outage` | healthy, then every request answers HTTP 500, then offline |
| `broken-release` | healthy, then a new `remoteEntry.js` whose chunks are missing, then offline |
| `broken-entry` | healthy, then a new `remoteEntry.js` that throws, then offline |
| `hang` | healthy, then servers that accept connections and never answer, then offline |
| `cold-offline` | offline on the first launch: the app must start and report cleanly |

Each launch is checked at one of three levels:

| Level | Passes when |
|---|---|
| `strict` | Every `--expect` text renders, the page settles, and it logged no errors. A broken release must roll back and reload, so the page that finally settles must be clean. |
| `render` | Every `--expect` text renders and the page settles. Errors are printed as notes. Used for the HTTP 500 and hanging launches. |
| `clean` | The app starts and reports without a Rust panic. Used for `cold-offline`, where there is nothing to render. |

### Running it

Build the remotes, then each host as an app bundle:

```bash
pnpm --filter example_guest --filter example-guest_2 --filter example_vite_guest build
(cd example/host && pnpm tauri build --bundles app)
(cd example/vite-host && pnpm tauri build --bundles app)
```

Then run the smoke test against each host:

```bash
pnpm e2e:smoke --app target/release/app \
  --cache "~/Library/Caches/dev.crabnebula.module-federation/module-federation" \
  --remote example/guest/dist:3002 --remote example/guest-2/dist:3003 \
  --expect "Host Button" --expect "Guest Button" --expect "Guest 2 Button"

pnpm e2e:smoke --app target/release/vite-host-app \
  --cache "~/Library/Caches/dev.module-federation.vite-host/module-federation" \
  --remote example/vite-guest/dist:3004 --remote example/guest/dist:3002 --remote example/guest-2/dist:3003 \
  --expect "Vite Guest Button" --expect "Guest Button" --expect "Guest 2 Button"
```

| Option | Meaning |
|---|---|
| `--app <binary>` | The built app to launch. |
| `--cache <dir>` | The plugin's cache folder for that app, cleared before each scenario. The paths above are the macOS locations. |
| `--remote <dist>:<port>` | Serve a built remote on a port. Repeatable. |
| `--expect <text>` | Text that must render. Repeatable. |
| `--scenario <name>` | Run only this scenario. Repeatable. |
| `--entry <file>` | The remote entry file name. Defaults to `remoteEntry.js`. |
| `--verbose` | Print page warnings and the requests each remote received. |

The script exits with code 1 if any launch fails. Without `--bundles app`, `tauri build` also builds a DMG, which needs a GUI session on macOS.

### The reporter

The example hosts include `example/tauri-e2e`, which is a no-op unless `MF_E2E` is set. With it set, the app:

1. Collects page errors (`error`, `unhandledrejection`, `console.error`) and `console.warn` messages.
2. Waits until the page has settled: text is present, none of it reads "Loading", and nothing has changed for 1.5 s. It gives up after `MF_E2E_WAIT_MS` milliseconds (default 20000). The deadline runs in Rust, because WebKit throttles timers in background windows.
3. Prints one `MF_E2E_REPORT {json}` line to stdout and exits.

The smoke test reads that line to decide whether each launch passed.
