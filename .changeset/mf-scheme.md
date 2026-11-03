---
'@module-federation/tauri': minor
---

Rename the custom protocol from `module-federation://` to `mf://`. `@module-federation/tauri` and `tauri-plugin-module-federation` must be updated together, because each version only understands its own scheme. The cache folder name is unchanged, so cached remotes still work offline after the update.
