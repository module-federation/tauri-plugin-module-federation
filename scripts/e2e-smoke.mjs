#!/usr/bin/env node
// End-to-end check of an example Tauri host against locally served remotes.
//
// Each scenario starts with an empty cache and launches the built app several times
// (a fresh process each time, like closing and reopening the app), with the remote
// servers in a different state for each launch:
//
//   restart         healthy, then offline
//   outage          healthy, then every request answers HTTP 500, then offline
//   broken-release  healthy, then a new remoteEntry.js whose chunks are missing, then offline
//   broken-entry    healthy, then a new remoteEntry.js that throws, then offline
//   hang            healthy, then servers that accept connections and never answer, then offline
//   cold-offline    offline on the very first launch: nothing to fall back to, the app
//                   must still start and report cleanly
//
// Usage:
//   node scripts/e2e-smoke.mjs --app <binary> --cache <dir> \
//     --remote <dist-dir>:<port> [--remote ...] --expect <text> [--expect ...] \
//     [--scenario <name> ...] [--verbose]

import { spawn } from 'node:child_process';
import { existsSync, readdirSync, readFileSync, rmSync, statSync } from 'node:fs';
import { createServer } from 'node:http';
import { homedir } from 'node:os';
import { extname, join, relative, resolve } from 'node:path';
import { parseArgs } from 'node:util';

const { values } = parseArgs({
	options: {
		app: { type: 'string' },
		cache: { type: 'string' },
		remote: { type: 'string', multiple: true, default: [] },
		expect: { type: 'string', multiple: true, default: [] },
		scenario: { type: 'string', multiple: true, default: [] },
		entry: { type: 'string', default: 'remoteEntry.js' },
		verbose: { type: 'boolean', default: false },
	},
});

if (!values.app || !values.cache) {
	console.error('--app and --cache are required');
	process.exit(2);
}

const ENTRY_PATH = `/${values.entry}`;
const BROKEN_ENTRY_MESSAGE = 'e2e: broken release';
const MIME = {
	'.js': 'text/javascript',
	'.mjs': 'text/javascript',
	'.css': 'text/css',
	'.html': 'text/html',
	'.json': 'application/json',
	'.svg': 'image/svg+xml',
};

const remotes = values.remote.map((spec) => {
	const i = spec.lastIndexOf(':');
	const root = resolve(spec.slice(0, i));
	return { root, port: Number(spec.slice(i + 1)), brokenEntry: missingChunksEntry(root) };
});

/** Files under `dir`, recursively. */
function walk(dir) {
	return readdirSync(dir, { withFileTypes: true }).flatMap((e) =>
		e.isDirectory() ? walk(join(dir, e.name)) : [join(dir, e.name)],
	);
}

/**
 * A "new release" of a remote: its remoteEntry.js points at chunk file names that do
 * not exist, as when a deploy uploads the entry but its chunks are missing.
 */
function missingChunksEntry(root) {
	let source = readFileSync(join(root, values.entry), 'utf8');
	let replaced = 0;
	for (const file of walk(root)) {
		if (relative(root, file) === values.entry) continue;
		const hash = /[.-]([A-Za-z0-9_]{8,})\.(?:m?js|css)$/.exec(file)?.[1];
		if (!hash || !source.includes(hash)) continue;
		const changed = [...hash].map((c) => (c === 'x' ? 'y' : 'x')).join('');
		source = source.split(hash).join(changed);
		replaced++;
	}
	if (replaced === 0) throw new Error(`no chunk references found in ${join(root, values.entry)}`);
	return source;
}

const MODES = {
	healthy: (remote) => (req, res, path) => serveFile(remote.root, path, res),
	'http-500': () => (req, res) => res.writeHead(500, { 'Content-Type': 'text/plain' }).end('Internal Server Error'),
	'missing-chunks': (remote) => (req, res, path) =>
		path === ENTRY_PATH ? sendJs(res, remote.brokenEntry) : serveFile(remote.root, path, res),
	'throwing-entry': (remote) => (req, res, path) =>
		path === ENTRY_PATH
			? sendJs(res, `throw new Error(${JSON.stringify(BROKEN_ENTRY_MESSAGE)});`)
			: serveFile(remote.root, path, res),
	hang: () => () => {},
};

function sendJs(res, source) {
	res.writeHead(200, { 'Content-Type': 'text/javascript', 'Access-Control-Allow-Origin': '*' }).end(source);
}

function serveFile(root, path, res) {
	const file = join(root, path.endsWith('/') ? `${path}index.html` : path);
	if (!file.startsWith(root) || !existsSync(file) || !statSync(file).isFile()) {
		res.writeHead(404).end();
		return;
	}
	res.writeHead(200, {
		'Content-Type': MIME[extname(file)] ?? 'application/octet-stream',
		'Access-Control-Allow-Origin': '*',
	});
	res.end(readFileSync(file));
}

const requestLog = [];

/** Starts every remote server in `mode`; `offline` starts none. Returns a stop function. */
async function startRemotes(mode) {
	if (mode === 'offline') return async () => {};
	const servers = await Promise.all(
		remotes.map((remote) => {
			const handle = MODES[mode](remote);
			const server = createServer((req, res) => {
				const path = decodeURIComponent(new URL(req.url, 'http://x').pathname);
				if (values.verbose) requestLog.push(`${remote.port} ${req.method} ${path}`);
				handle(req, res, path);
			});
			return new Promise((ok, fail) => {
				server.once('error', fail);
				server.listen(remote.port, () => ok(server));
			});
		}),
	);
	return () =>
		Promise.all(servers.map((s) => new Promise((ok) => (s.close(ok), s.closeAllConnections()))));
}

function runApp(waitMs) {
	return new Promise((ok) => {
		const child = spawn(resolve(values.app), [], {
			env: { ...process.env, MF_E2E: '1', MF_E2E_WAIT_MS: String(waitMs) },
			stdio: ['ignore', 'pipe', 'pipe'],
		});
		let out = '';
		child.stdout.on('data', (d) => (out += d));
		child.stderr.on('data', (d) => (out += d));
		const timer = setTimeout(() => child.kill('SIGKILL'), waitMs + 20_000);
		child.on('exit', () => {
			clearTimeout(timer);
			const line = out.split('\n').find((l) => l.startsWith('MF_E2E_REPORT '));
			ok({ report: line ? JSON.parse(line.slice('MF_E2E_REPORT '.length)) : null, out });
		});
	});
}

// Phases: [label, remote server mode, what must hold].
//   render: every --expect text is shown and the page settles.
//   strict: additionally, the page logged no errors. A broken release is expected to
//           roll back and reload, so the page that finally settles must be clean.
//   clean:  the app starts and reports, without a Rust panic (nothing to render).
const SCENARIOS = {
	restart: [
		['healthy', 'healthy', 'strict'],
		['offline restart', 'offline', 'strict'],
	],
	outage: [
		['healthy', 'healthy', 'strict'],
		['remotes answer HTTP 500', 'http-500', 'render'],
		['offline restart', 'offline', 'strict'],
	],
	'broken-release': [
		['healthy', 'healthy', 'strict'],
		['new release with missing chunks', 'missing-chunks', 'strict'],
		['offline restart', 'offline', 'strict'],
	],
	'broken-entry': [
		['healthy', 'healthy', 'strict'],
		['new remoteEntry.js throws', 'throwing-entry', 'strict'],
		['offline restart', 'offline', 'strict'],
	],
	hang: [
		['healthy', 'healthy', 'strict'],
		['remotes never answer', 'hang', 'render'],
		['offline restart', 'offline', 'strict'],
	],
	'cold-offline': [['first launch offline', 'offline', 'clean']],
};

function check(expectation, { report, out }) {
	const problems = [];
	const notes = [];
	if (/panicked at/.test(out)) problems.push(`Rust panic:\n${out.trim()}`);
	if (!report) {
		problems.push(`no report (app output:\n${out.trim()})`);
		return { problems, notes };
	}
	if (expectation === 'clean') return { problems, notes };

	if (!report.settled) problems.push('page did not settle');
	for (const text of values.expect) {
		if (!report.text.includes(text)) problems.push(`missing text: ${JSON.stringify(text)}`);
	}
	if (values.verbose) for (const w of report.warnings ?? []) notes.push(`warn: ${w}`);
	for (const error of report.errors) {
		(expectation === 'strict' ? problems : notes).push(values.verbose ? error : error.split('\n')[0].slice(0, 200));
	}
	return { problems, notes };
}

const cacheDir = values.cache.replace(/^~/, homedir());
const scenarios = values.scenario.length ? values.scenario : Object.keys(SCENARIOS);
const results = [];

for (const name of scenarios) {
	const phases = SCENARIOS[name];
	if (!phases) throw new Error(`unknown scenario ${name}; known: ${Object.keys(SCENARIOS).join(', ')}`);
	rmSync(cacheDir, { recursive: true, force: true });
	console.log(`\n### ${name}`);

	for (const [label, mode, expectation] of phases) {
		const stop = await startRemotes(mode);
		const started = Date.now();
		const run = await runApp(mode === 'hang' ? 60_000 : 20_000);
		await stop();

		const { problems, notes } = check(expectation, run);
		const seconds = ((Date.now() - started) / 1000).toFixed(1);
		results.push({ name, label, ok: problems.length === 0 });
		console.log(`  ${problems.length ? 'FAIL' : 'PASS'}  ${label} (${seconds}s)`);
		for (const p of problems) console.log(`        ! ${p.replace(/\n/g, '\n          ')}`);
		for (const n of notes) console.log(`        · page logged: ${n}`);
		if (problems.length && run.report) console.log(run.report.text.replace(/^/gm, '        | '));
		if (values.verbose) for (const line of requestLog.splice(0)) console.log(`        > ${line}`);
	}
}

const failed = results.filter((r) => !r.ok);
console.log(`\n${results.length - failed.length}/${results.length} phases passed`);
process.exitCode = failed.length ? 1 : 0;
