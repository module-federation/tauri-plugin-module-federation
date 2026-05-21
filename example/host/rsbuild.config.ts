import { pluginModuleFederation } from '@module-federation/rsbuild-plugin';
import { defineConfig } from '@rsbuild/core';
import { pluginReact } from '@rsbuild/plugin-react';
import { withZephyr } from 'zephyr-rsbuild-plugin';

const useZephyr = process.env.WITH_ZEPHYR === 'true';

export default defineConfig({
	plugins: [
		pluginReact(),
		pluginModuleFederation(
			{
				name: 'example_host',
				filename: 'remoteEntry.js',
				remotes: {
					example_guest:
						'example_guest@http://localhost:3002/remoteEntry.js',
				},
				runtimePlugins: ['@module-federation/tauri'],
				shared: {
					react: { eager: true },
					'react-dom': { eager: true },
					lodash: {},
				},
			},
			{},
		),
		useZephyr && withZephyr(),
	].filter(Boolean),
	dev: {
		assetPrefix: 'auto',
	},
	output: {
		assetPrefix: 'auto',
	},
	server: {
		port: 3001,
	},
	html: {
		template: './public/index.html',
	},
});
