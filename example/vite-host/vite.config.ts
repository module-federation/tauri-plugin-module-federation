import { federation } from '@module-federation/vite';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

export default defineConfig({
	plugins: [
		react(),
		federation({
			name: 'example_vite_host',
			filename: 'remoteEntry.js',
			// The examples are plain JavaScript, so there are no types to generate.
			dts: false,
			remotes: {
				// Vite remote: ES module remoteEntry.
				example_vite_guest: {
					type: 'module',
					name: 'example_vite_guest',
					entry: 'http://localhost:3004/remoteEntry.js',
				},
				// Rsbuild remote: classic `var` remoteEntry.
				example_guest: {
					type: 'var',
					name: 'example_guest',
					entry: 'http://localhost:3002/remoteEntry.js',
					entryGlobalName: 'example_guest',
				},
			},
			runtimePlugins: ['@module-federation/tauri'],
			shared: {
				react: { singleton: true },
				'react-dom': { singleton: true },
				lodash: {},
			},
		}),
	],
	// Tauri expects a fixed port and fails if it is not available.
	server: {
		port: 3005,
		strictPort: true,
	},
	clearScreen: false,
	build: {
		target: 'esnext',
	},
});
