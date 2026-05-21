---
title: Releasing
read_when:
  - preparing a release
  - updating release automation
---

# Releasing

This repo publishes:

- npm package: `@module-federation/tauri`
- cargo crate: `tauri-plugin-module-federation`

## Release flow

1. Bump `module-federation-plugin/package.json` to the release version.
2. Run `pnpm release:sync` to sync the Cargo crate version.
3. Commit the version change.
4. Cut a GitHub Release from that version commit.

The `Release` workflow publishes both npm and crates.io from the same version.

## Required secrets

- `NPM_TOKEN`
- `CARGO_REGISTRY_TOKEN`

## Tag format

Use plain semver tags.

- valid: `1.2.3`
- invalid: `v1.2.3`

## Notes

- `release.yml` syncs the Cargo crate version from `module-federation-plugin/package.json` before publish.
- `pnpm build` verifies both publish artifacts before release: `rslib build` for npm and `cargo package` for crates.io.
- `workflow_dispatch` with `version=next` creates a snapshot-style prerelease version for npm and crates.io.
- Changesets remain available for local versioning if you want to use `changeset version` before cutting a release.
