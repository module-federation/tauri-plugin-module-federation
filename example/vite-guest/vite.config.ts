import { federation } from '@module-federation/vite';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

export default defineConfig({
	// A relative base keeps chunk URLs relative to remoteEntry.js, so they are
	// requested through the mf:// protocol and cached for offline use.
	// An absolute base (e.g. 'http://localhost:3004/') would bypass the plugin.
	base: './',
	plugins: [
		react(),
		federation({
			name: 'example_vite_guest',
			filename: 'remoteEntry.js',
			// The examples are plain JavaScript, so there are no types to generate.
			dts: false,
			exposes: {
				'./Button': './src/Button.jsx',
			},
			remotes: {
				example_guest_2: {
					type: 'var',
					name: 'example_guest_2',
					entry: 'http://localhost:3003/remoteEntry.js',
					entryGlobalName: 'example_guest_2',
				},
			},
			shared: {
				react: { singleton: true },
				'react-dom': { singleton: true },
				lodash: {},
			},
		}),
	],
	server: {
		port: 3004,
		strictPort: true,
		origin: 'http://localhost:3004',
	},
	preview: {
		port: 3004,
		strictPort: true,
		// ES module remotes are loaded in CORS mode. A release Tauri build has the
		// origin tauri://localhost (http://tauri.localhost on Windows), which Vite's
		// default localhost-only CORS policy rejects.
		cors: true,
	},
	build: {
		target: 'esnext',
	},
});
