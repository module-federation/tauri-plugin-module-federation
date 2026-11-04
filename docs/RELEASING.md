---
title: Releasing
summary: Release and publishing for @module-federation/tauri (npm) and tauri-plugin-module-federation (crates.io).
read_when:
  - preparing a release
  - updating release automation
---

# Releasing

This repo publishes two artifacts with the same version:

| Artifact | Registry | Source |
|---|---|---|
| `@module-federation/tauri` | npm | `packages/tauri` |
| `tauri-plugin-module-federation` | crates.io | `packages/tauri-plugin` |

Releases are cut by [release-please](https://github.com/googleapis/release-please) from Conventional Commits on `main`. PRs are squash-merged with the PR title as the commit message, and `Validate PR title` enforces the format, so the PR title decides the release:

- `fix:` (and `perf:`) releases a patch.
- `feat:` releases a minor.
- `feat!:` or a `BREAKING CHANGE:` footer releases a minor while the version is below 1.0 (`bump-minor-pre-major`), then a major.
- `chore:`, `docs:`, `test:`, `ci:`, `refactor:`, and `build:` release nothing and stay out of the changelog.

`CI` runs this gate on every PR and on `main`. Run it locally before merging:

```bash
pnpm install --frozen-lockfile
pnpm release:sync:check
cargo fmt --all --check
pnpm build
cargo test -p tauri-plugin-module-federation
pnpm pack:tauri
```

`pnpm build` builds the npm package and the example apps (including `tauri build` for the hosts), then runs `cargo package` on the crate. On Linux, Tauri needs its [system dependencies](https://v2.tauri.app/start/prerequisites/#linux).

`pkg-pr-new` publishes a preview of `@module-federation/tauri` to [pkg.pr.new](https://pkg.pr.new) for every PR and push to `main`, so a change can be tried before it is released: `npm i https://pkg.pr.new/@module-federation/tauri@<PR number>`.

## Release PR

On every push to `main`, the `Release Please` workflow opens or updates a `chore: release <version>` PR. It bumps:

- `packages/tauri/package.json`
- `packages/tauri-plugin/Cargo.toml` and the crate's entry in `Cargo.lock`
- `.release-please-manifest.json`
- `packages/tauri/CHANGELOG.md`

`release-please-config.json` lists the Cargo files under `extra-files`, so the crate version always matches the npm version. Edit the PR's changelog entry before merging if the commit titles need context. To force a version, add a `Release-As: x.y.z` footer to a commit on `main`.

Merging the release PR makes release-please tag the merge commit and publish a GitHub Release named after the version, without a `v` (`0.1.0`, not `v0.1.0`). That release triggers `Release`, which publishes both artifacts. npm gets the `latest` dist-tag.

release-please authenticates as a GitHub App (`RELEASE_APP_ID` and `RELEASE_APP_PRIVATE_KEY`) with contents, issues and pull-request write access. Releases created with the default `GITHUB_TOKEN` would not trigger the publish workflow.

## Publish workflow

`Release` runs on published GitHub Releases and on manual dispatch. It:

1. checks that the source is reachable from `main`;
2. for releases, checks that the tag equals `packages/tauri/package.json`'s version and that the Cargo version matches (`pnpm release:sync:check`);
3. runs `pnpm build` and `pnpm pack:tauri`;
4. publishes to npm with provenance, and refuses to republish an existing version under a different dist-tag;
5. publishes to crates.io unless that version already exists there.

Re-running it for a version that is already on both registries does nothing.

Dispatching `Release` with `version=next` publishes a preview: the next patch version with a `-next.<run id>` suffix, to npm's `next` dist-tag and to crates.io as a prerelease. Dispatching with `version=latest` republishes the committed version and is only needed when a release-triggered publish failed.

GitHub prerelease tags must match the prerelease version already committed in `packages/tauri/package.json`. The workflow publishes that exact version to the `next` dist-tag.

## Repository setup

- A `Publish` environment. The publish job runs in it.
- `RELEASE_APP_ID` and `RELEASE_APP_PRIVATE_KEY`: the GitHub App release-please uses.
- `CARGO_REGISTRY_TOKEN`: a crates.io API token with publish rights for `tauri-plugin-module-federation`.

npm needs no token. The workflow uses npm trusted publishing for repository `module-federation/tauri-plugin-module-federation`, workflow `release.yml`, environment `Publish`.

## First publish

npm only lets a package that already exists name a trusted publisher, so the workflow cannot publish `@module-federation/tauri` the first time. Bootstrap it once, as a maintainer with publish rights in the `@module-federation` npm organization:

1. Publish a placeholder from an empty directory: a `package.json` with `"name": "@module-federation/tauri"` and `"version": "0.0.0"`, then `npm publish --access public`.
2. Add the trusted publisher on npmjs.com (package settings, "Trusted Publisher", GitHub Actions) or with npm 11.10+:
   `npm trust github @module-federation/tauri --file release.yml --repo module-federation/tauri-plugin-module-federation --env Publish --allow-publish`.
3. Release the first version through the workflow within two days: npm discards a trusted publisher configuration that has not published by then.
4. Deprecate the placeholder:
   `npm deprecate @module-federation/tauri@0.0.0 "Placeholder; use a later version."`

Trusted publishing needs npm 11.5.1+ on Node 22.14+; the workflow's Node 24 includes it.

crates.io needs no bootstrap: `CARGO_REGISTRY_TOKEN` creates the crate on its first publish.
