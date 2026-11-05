import lodash from 'lodash';
import React, { version as reactVersion } from 'react';
import ViteGuestButton from 'example_vite_guest/Button';
import RsbuildGuestButton from 'example_guest/Button';

const App = () => (
	<div>
		<h1>Module Federation in Tauri (Vite host)</h1>
		<h4>Host Used React: {reactVersion}</h4>
		<h4>Host Used Lodash: {lodash.VERSION}</h4>

		<h3>Vite remote</h3>
		<React.Suspense fallback="Loading Vite Guest Button">
			<ViteGuestButton />
		</React.Suspense>

		<h3>Rsbuild remote</h3>
		<React.Suspense fallback="Loading Guest Button">
			<RsbuildGuestButton />
		</React.Suspense>
	</div>
);

export default App;
