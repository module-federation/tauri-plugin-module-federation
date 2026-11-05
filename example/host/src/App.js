import React, { version as reactVersion } from 'react';
import { version as reactDomVersion } from 'react-dom';
import lodash from 'lodash';

import { loadRemote, registerRemotes } from '@module-federation/runtime';

import LocalButton from './Button';
import RemoteButton from 'example_guest/Button';

// Rsbuild loads remotes declared in rsbuild.config.ts as classic `script` remotes.
// An ES module remote, such as one built with @module-federation/vite, is registered
// on the host's runtime instance instead, so it also goes through
// @module-federation/tauri and is cached by the Tauri plugin.
registerRemotes([
	{
		name: 'example_vite_guest',
		type: 'module',
		entry: 'http://localhost:3004/remoteEntry.js',
	},
]);
const ViteGuestButton = React.lazy(() => loadRemote('example_vite_guest/Button'));

const getColorFromString = (str) => {
	const primes = [1, 2, 3, 5, 7, 11, 13, 17, 19, 23];
	let hash = 0;
	for (let i = 0; i < str.length; i++) {
		hash += str.charCodeAt(i) * primes[i % primes.length];
	}
	let color = '#';
	for (let i = 0; i < 3; i++) {
		const value = (hash >> (i * 8)) & 0xff;
		color += ('00' + value.toString(16)).substr(-2);
	}
	return color;
};

const App = () => (
	<div>
		<h1>Module Federation in Tauri (Rsbuild host)</h1>
		<h4 style={{ color: getColorFromString(reactVersion) }}>
			Host Used React: {reactVersion}
		</h4>
		<h4 style={{ color: getColorFromString(reactDomVersion) }}>
			Host Used ReactDOM: {reactDomVersion}
		</h4>
		<h4 style={{ color: getColorFromString(lodash.VERSION) }}>
			Host Used Lodash: {lodash.VERSION}
		</h4>

		<LocalButton />

		<h3>Rsbuild remote</h3>
		<React.Suspense fallback="Loading Guest Button">
			<RemoteButton />
		</React.Suspense>

		<h3>Vite remote</h3>
		<React.Suspense fallback="Loading Vite Guest Button">
			<ViteGuestButton />
		</React.Suspense>
	</div>
);

export default App;
