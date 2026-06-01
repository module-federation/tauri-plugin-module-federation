import { defineConfig } from '@rslib/core';

export default defineConfig({
	source: {
		entry: {
			index: './index.js',
		},
	},
	lib: [
		{
			format: 'esm',
			dts: true,
		},
	],
	output: {
		target: 'web',
	},
});
