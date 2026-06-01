import { registerGlobalPlugins } from "@module-federation/runtime";

/**
 * @typedef {Parameters<NonNullable<import("@module-federation/runtime").ModuleFederationRuntimePlugin["afterResolve"]>>[0]} LoadRemoteMatch
 */

/**
    @param {LoadRemoteMatch} args
 */
function afterResolve(args) {
	const url = new URL(args.remoteInfo.entry);
	args.remoteInfo.entry = `module-federation://${url.host}/?fullUrl=${url}`;
	return args;
}

const hostPlugin = () => {
	/** @type {import("@module-federation/runtime").ModuleFederationRuntimePlugin} */
	const plugin = {
		name: "tauri-module-federation-host",
		afterResolve,
		beforeInit: (args) => {
			registerGlobalPlugins([
				{ name: "tauri-module-federation-global", afterResolve },
			]);
			return args;
		},
	};
	return plugin;
};

export default hostPlugin;
