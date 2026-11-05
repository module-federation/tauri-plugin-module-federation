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

Both artifacts share one version, so a change to either package releases both. release-please tracks the repo root and ignores commits that only touch `.github`, `docs`, `example` or `scripts`: a `fix:` to the Rust crate releases, a `feat:` to an example app does not.

`CI` runs this gate on every PR and on `main`. Run it locally before merging:

```bash
pnpm install --frozen-lockfile
pnpm release:sync:check
cargo fmt --all --check
pnpm build:ci
cargo test -p tauri-plugin-module-federation
pnpm pack:tauri
```

`pnpm build:ci` builds the npm package, all remote examples, and both Tauri hosts with `tauri build --no-bundle`, then runs `cargo package` on the crate. CI verifies the host executables without generating example installers, so AppImage packaging does not block SDK releases. `pnpm build` still generates example installers for local use. On Linux, Tauri needs its [system dependencies](https://v2.tauri.app/start/prerequisites/#linux).

`pkg-pr-new` publishes a preview of `@module-federation/tauri` to [pkg.pr.new](https://pkg.pr.new) for every PR and push to `main`, so a change can be tried before it is released: `npm i https://pkg.pr.new/@module-federation/tauri@<PR number>`.

## Release PR

On every push to `main`, the `Release Please` workflow opens or updates a `chore: release <version>` PR. It bumps:

- `packages/tauri/package.json`
- `packages/tauri-plugin/Cargo.toml` and the crate's entry in `Cargo.lock`
- `.release-please-manifest.json`
- `packages/tauri/CHANGELOG.md`

`release-please-config.json` lists `packages/tauri/package.json` and the Cargo files under `extra-files`, so the crate version always matches the npm version. Edit the PR's changelog entry before merging if the commit titles need context. To force a version, add a `Release-As: x.y.z` footer to a commit on `main`.

Merging the release PR makes release-please tag the merge commit and publish a GitHub Release named after the version, without a `v` (`0.1.0`, not `v0.1.0`). Release Please then explicitly dispatches `Release` with that exact tag, which publishes both artifacts when Cargo publishing is enabled. npm gets the `latest` dist-tag.

release-please uses the repository's default `GITHUB_TOKEN`, with contents, issues, pull-request and actions write permissions. No GitHub App secrets or personal access token are required. A release created with `GITHUB_TOKEN` does not trigger a `release` event workflow, so the explicit `workflow_dispatch` is required; dispatch events can start another workflow using `GITHUB_TOKEN`.

PRs created or updated by `GITHUB_TOKEN` do not automatically trigger pull-request workflows either. If branch protection requires PR checks, a maintainer must trigger the checks or push a change to the release PR before merging.

## Publish workflow

`Release` runs on published GitHub Releases and on manual dispatch. It:

1. checks that the source is reachable from `main`;
2. for releases, checks that the tag equals `packages/tauri/package.json`'s version and that the Cargo version matches (`pnpm release:sync:check`);
3. checks synchronized versions and Rust formatting, runs `pnpm build:ci`, the Rust tests, and `pnpm pack:tauri`;
4. publishes to npm through OIDC (which generates provenance automatically), and refuses to republish an existing version under a different dist-tag;
5. when repository variable `CARGO_PUBLISH_ENABLED` is `true`, runs a separate crates.io job using the exact same source commit and version, verifies the Cargo package, and publishes through OIDC unless that version already exists there.

Re-running it for a version that is already on both registries does nothing.

Dispatching `Release` with `version=next` publishes a preview: the next patch version with a `-next.<run id>` suffix, to npm's `next` dist-tag and, when Cargo publishing is enabled, to crates.io as a prerelease. Dispatching with `version=latest` republishes the committed version and is only needed when a release-triggered publish failed.

The optional `release_tag` input publishes the exact tagged source instead of the current `main` commit and checks the tag against the package version. Release Please always supplies it. It can only be used with `version=latest`; prerelease versions still go to the `next` dist-tag.

GitHub prerelease tags must match the prerelease version already committed in `packages/tauri/package.json`. The workflow publishes that exact version to the `next` dist-tag.

To validate npm OIDC from a PR before merging, dispatch `release.yml` on the PR head branch with `version=next` and `branch` set to that same branch. The preview branch must contain the current `main`. Branch previews only publish npm's `next` dist-tag; Cargo publishing runs only when the workflow runs on `main`. Stable releases and exact release tags remain main-only.

## Repository setup

- A `Publish` environment. The publish job runs in it.
- Enable **Allow GitHub Actions to create and approve pull requests** under the repository's Actions workflow permissions. Release Please needs this setting to create its release PR with `GITHUB_TOKEN`; the workflow does not approve PRs.
- If the organization disables this permission, an organization owner must allow it before it can be enabled for the repository.
- `CARGO_PUBLISH_ENABLED=true`, only after the crate owner completes the crates.io setup below.

Both publish jobs use GitHub-hosted runners with `contents: read` and `id-token: write`. Neither npm nor crates.io needs a long-lived publishing token in GitHub secrets. The Rust job uses `rust-lang/crates-io-auth-action`, which retrieves a temporary token and revokes it when the job completes.

Both registries must trust repository `module-federation/tauri-plugin-module-federation`, workflow `release.yml`, environment `Publish`. If the repository is renamed, update both trusted publisher configurations and the package repository metadata before publishing again.

## First publish

npm only lets a package that already exists name a trusted publisher, so the workflow cannot publish `@module-federation/tauri` the first time. Bootstrap it once, as a maintainer with publish rights in the `@module-federation` npm organization:

1. Build and inspect the current `0.0.0` package with `pnpm build:package` and `pnpm pack:tauri`, then publish it once from `packages/tauri` with `npm publish --access public`. This bootstrap release contains the runtime plugin and declarations.
2. Add the trusted publisher on npmjs.com (package settings, "Trusted Publisher", GitHub Actions) or with npm 11.10+:
   `npm trust github @module-federation/tauri --file release.yml --repo module-federation/tauri-plugin-module-federation --env Publish --allow-publish`.
3. Publish the next version through the workflow before the validation deadline shown in npm package settings. An unvalidated connection expires after two days. The connection created on November 5, 2026 must publish before November 7, 2026 at 11:51 UTC (19:51 China Standard Time).

Trusted publishing needs npm 11.5.1+ on Node 22.14+. The workflow uses Node 24 and explicitly installs npm 11.10+.

## crates.io setup

The maintainer can use their own crates.io account. Crate names are global; no organization scope is required.

1. Publish `tauri-plugin-module-federation` once using the maintainer's local Cargo credentials, after `cargo publish --dry-run --manifest-path packages/tauri-plugin/Cargo.toml`.
2. Add another maintainer or a GitHub team as a crate owner:
   `cargo owner --add github:module-federation:<team-slug> tauri-plugin-module-federation`.
3. In the crate's crates.io settings, add a GitHub Actions trusted publisher with repository owner `module-federation`, repository `tauri-plugin-module-federation`, workflow filename `release.yml`, and environment `Publish`.
4. Set repository variable `CARGO_PUBLISH_ENABLED` to `true` only after trusted publishing is configured.

Until then, the npm release works independently and the Cargo publish job is skipped. The crate owner can also publish Rust releases manually. A Cargo job failure does not undo an npm release; rerun the same workflow run to retry the original commit and version.
