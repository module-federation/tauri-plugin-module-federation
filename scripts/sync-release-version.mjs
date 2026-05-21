import { readFileSync, writeFileSync } from 'node:fs';

const checkOnly = process.argv.includes('--check');
const packageJsonPath = new URL('../module-federation-plugin/package.json', import.meta.url);
const cargoTomlPath = new URL('../tauri-plugin/Cargo.toml', import.meta.url);
const cargoLockPath = new URL('../Cargo.lock', import.meta.url);

const packageJson = JSON.parse(readFileSync(packageJsonPath, 'utf8'));
const version = packageJson.version;

if (!version) {
	throw new Error('module-federation-plugin/package.json is missing a version');
}

const replaceVersion = (source, pattern, label) => {
	if (!pattern.test(source)) {
		throw new Error(`Could not find ${label} to sync version`);
	}
	return source.replace(pattern, version);
};

const cargoToml = readFileSync(cargoTomlPath, 'utf8');
const nextCargoToml = replaceVersion(
	cargoToml,
	/(?<=^version = ")([^"]+)(?="$)/m,
	'tauri-plugin/Cargo.toml version',
);

const cargoLock = readFileSync(cargoLockPath, 'utf8');
const nextCargoLock = replaceVersion(
	cargoLock,
	/(?<=\[\[package\]\]\nname = "tauri-plugin-module-federation"\nversion = ")([^"]+)(?=")/m,
	'Cargo.lock tauri-plugin-module-federation version',
);

const changed = cargoToml !== nextCargoToml || cargoLock !== nextCargoLock;

if (checkOnly) {
	if (changed) {
		console.error(`Release versions are out of sync. Expected ${version}.`);
		process.exit(1);
	}
	process.exit(0);
}

if (cargoToml !== nextCargoToml) {
	writeFileSync(cargoTomlPath, nextCargoToml);
}

if (cargoLock !== nextCargoLock) {
	writeFileSync(cargoLockPath, nextCargoLock);
}

console.log(`Synced Cargo release version to ${version}`);
