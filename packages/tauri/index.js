import { registerGlobalPlugins } from "@module-federation/runtime";

/**
 * @typedef {import("@module-federation/runtime").ModuleFederationRuntimePlugin} RuntimePlugin
 * @typedef {Parameters<NonNullable<RuntimePlugin["afterResolve"]>>[0]} LoadRemoteMatch
 * @typedef {Parameters<NonNullable<RuntimePlugin["errorLoadRemote"]>>[0]} ErrorLoadRemoteArgs
 * @typedef {Parameters<NonNullable<RuntimePlugin["onLoad"]>>[0]} OnLoadArgs
 */

const PROTOCOL = "mf://";
/** Tell the Tauri plugin that an entry loaded, so it keeps that version as last-known-good. */
const COMMIT_PARAM = "mf-commit";
/** Ask the Tauri plugin to reject this launch's new, unconfirmed entry versions. */
const ROLLBACK_URL = `${PROTOCOL}localhost/?mf-rollback=1`;
const LOG_PREFIX = "[@module-federation/tauri]";

/**
 * Rewrites an http(s) URL to the `mf://` protocol. The path is kept so that relative
 * chunk URLs resolve under it, and the original URL travels encoded in `fullUrl`.
 * @param {string} entry
 * @param {Record<string, string>} [params]
 */
function toProtocolUrl(entry, params = {}) {
	const url = new URL(entry);
	const query = new URLSearchParams({ fullUrl: url.href, ...params });
	return `${PROTOCOL}${url.host}${url.pathname}?${query}`;
}

/**
 * Route remote entries through the Tauri plugin.
 * @param {LoadRemoteMatch} args
 */
function afterResolve(args) {
	const { entry } = args.remoteInfo;
	// Only http(s) entries are rewritten. Entries already on mf:// (the host and global
	// plugins can both run on one instance) and other schemes pass through unchanged.
	if (/^https?:\/\//i.test(entry)) args.remoteInfo.entry = toProtocolUrl(entry);
	return args;
}

/** Entries already committed during this page load. */
const committed = new Set();

/**
 * A remote loaded: the version of its entry that was served is known good.
 * @param {OnLoadArgs} args
 */
function onLoad(args) {
	const entry = args.moduleInstance?.remoteInfo?.entry;
	if (!entry?.startsWith(PROTOCOL) || committed.has(entry)) return;
	committed.add(entry);
	fetch(`${entry}&${COMMIT_PARAM}=1`).catch((error) =>
		console.warn(`${LOG_PREFIX} Could not mark ${entry} as working.`, error),
	);
}

let rollback = null;

/**
 * A remote failed to load. If this launch served new versions of remote entries that
 * have not loaded successfully yet, one of them is likely the cause: the Tauri plugin
 * rejects them, and the page reloads with the last versions that worked. Reloading
 * also resets shared modules a broken release may have registered.
 * @param {ErrorLoadRemoteArgs} args
 */
async function errorLoadRemote(args) {
	if (args.lifecycle !== "onLoad" && args.lifecycle !== "beforeLoadShare") return;

	rollback ??= fetch(ROLLBACK_URL)
		.then((response) => response.text())
		.then(Number)
		.catch((error) => {
			console.warn(`${LOG_PREFIX} Could not roll back remotes.`, error);
			return 0;
		});
	const rolledBack = await rollback;
	rollback = null;

	// The Tauri plugin caps rollbacks per launch, so this cannot reload forever.
	if (rolledBack > 0) {
		console.warn(
			`${LOG_PREFIX} A remote failed to load. Reloading with the last versions that worked (${rolledBack} rolled back).`,
			args.error,
		);
		location.reload();
		// Keep the failed load pending until the page reloads, instead of surfacing the error.
		return new Promise(() => {});
	}
	return undefined;
}

/** @type {Omit<RuntimePlugin, "name">} */
const hooks = { afterResolve, onLoad, errorLoadRemote };

const hostPlugin = () => {
	/** @type {RuntimePlugin} */
	const plugin = {
		name: "tauri-module-federation-host",
		...hooks,
		beforeInit: (args) => {
			// Remotes create their own runtime instances for nested remotes; cover those too.
			registerGlobalPlugins([{ name: "tauri-module-federation-global", ...hooks }]);
			return args;
		},
	};
	return plugin;
};

export default hostPlugin;
