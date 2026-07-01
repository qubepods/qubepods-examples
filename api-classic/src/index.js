// API-Classic — a classic HTTP API worker, deployed to qubepods with `qube deploy`,
// using KV + SQLite + R2 with ZERO wrangler config.
//
// The point of this example: you write an ordinary `export default { fetch }`
// module (no wasm on the server). You do NOT declare any Cloudflare bindings, ids,
// or namespaces. The platform reads the `env` block in qube.json5 and injects
// three reserved bindings into `env` at deploy time (docs: qubepods §"Storage
// tiers, reserved bindings"):
//
//   env.KV      — key-value       (service binding to the platform KV gateway,
//                                   backed by this project's Durable Object)
//   env.DB      — SQLite database (light-fast tier: the SAME project Durable
//                                   Object's SQLite, via the SQL gateway —
//                                   `env.DB.exec/query/first`)
//   env.BUCKET  — object storage  (a dedicated R2 bucket, this project's bucket)
//
// One of each, per project, under fixed names — so any app in the project that
// declares the same env block binds the SAME stores. KV and the light-fast DB both
// live in ONE per-project durable, so the twin app (front-end wasm rendering a
// QView, back-end wasm) reading `env.kv`/`env.db` shares these exact stats — same
// project, same durable, same tables.
//
// The test page is served by THIS worker (from the API, not as a static asset):
// GET / returns an HTML dashboard that calls the JSON endpoints below and shows,
// per binding, whether it was injected and whether a real round-trip works.

const DB_TABLE = 'events'; // one row per recorded run — the shared "stats" table
const R2_KEY = 'stats/last-run.json'; // the shared "last run" object in R2
const KV_VISITS = 'stats:visits'; // a shared counter in KV

export default {
	async fetch(request, env) {
		const url = new URL(request.url);
		try {
			switch (url.pathname) {
				case '/':
					return html(PAGE);
				case '/api/health':
					return json(health(env));
				case '/api/tests':
					return json(await runTests(env));
				case '/api/stats':
					return json(await readStats(env));
				default:
					return json({ error: 'not found', path: url.pathname }, 404);
			}
		} catch (e) {
			return json({ error: String(e && e.message ? e.message : e) }, 500);
		}
	}
};

// --- binding presence -------------------------------------------------------
// Proof the platform injected the bindings: a raw JS worker never declared them.
function health(env) {
	return {
		ok: true,
		note: 'these bindings were injected by qubepods from the qube.json5 env block — no wrangler.jsonc',
		bindings: {
			KV: hasMethod(env.KV, 'get') && hasMethod(env.KV, 'put'),
			DB: hasMethod(env.DB, 'exec') && hasMethod(env.DB, 'query'),
			BUCKET: hasMethod(env.BUCKET, 'put') && hasMethod(env.BUCKET, 'get')
		}
	};
}

function hasMethod(obj, name) {
	return !!obj && typeof obj[name] === 'function';
}

// --- the tests --------------------------------------------------------------
// Each binding does a real round-trip and reports pass/fail + a short detail.
// A missing binding is reported as skipped (not a hard failure), so the page is
// honest about exactly what the platform wired up.
async function runTests(env) {
	const results = [];
	results.push(await kvTest(env));
	results.push(await dbTest(env));
	results.push(await r2Test(env));
	const ran = results.filter((r) => r.status !== 'skipped');
	return {
		ok: ran.every((r) => r.ok),
		passed: ran.filter((r) => r.ok).length,
		total: ran.length,
		results,
		stats: await readStats(env)
	};
}

// KV: bump a shared visit counter, then write + read + list a scratch key.
async function kvTest(env) {
	if (!hasMethod(env.KV, 'put')) return skip('KV', 'env.KV not injected');
	try {
		const visits = (parseInt((await env.KV.get(KV_VISITS)) || '0', 10) || 0) + 1;
		await env.KV.put(KV_VISITS, String(visits));

		const key = 'scratch:hello';
		await env.KV.put(key, 'world', { metadata: { at: Date.now() } });
		const value = await env.KV.get(key);
		const listed = await env.KV.list({ prefix: 'scratch:' });
		const found = (listed.keys || []).some((k) => k.name === key);

		const ok = value === 'world' && found;
		return result('KV', ok, `put/get="${value}", list found=${found}, visits=${visits}`);
	} catch (e) {
		return fail('KV', e);
	}
}

// SQLite (light-fast, project DO): create the shared events table, insert a row,
// count the rows — via the SQL gateway's flat exec/first surface.
async function dbTest(env) {
	if (!hasMethod(env.DB, 'exec')) return skip('DB', 'env.DB not injected');
	try {
		await env.DB.exec(
			`CREATE TABLE IF NOT EXISTS ${DB_TABLE} (id INTEGER PRIMARY KEY AUTOINCREMENT, kind TEXT, at INTEGER)`
		);
		await env.DB.exec(`INSERT INTO ${DB_TABLE} (kind, at) VALUES (?, ?)`, 'run', Date.now());
		const row = await env.DB.first(`SELECT count(*) AS n FROM ${DB_TABLE}`);
		const n = row ? row.n : 0;
		return result('DB', n > 0, `SQLite (light-fast, project DO): ${n} row(s) in ${DB_TABLE}`);
	} catch (e) {
		return fail('DB', e);
	}
}

// R2: write the shared last-run object, read it back, and list the prefix.
async function r2Test(env) {
	if (!hasMethod(env.BUCKET, 'put')) return skip('BUCKET', 'env.BUCKET not injected');
	try {
		const body = JSON.stringify({ at: new Date().toISOString(), by: 'api-classic' });
		await env.BUCKET.put(R2_KEY, body, { httpMetadata: { contentType: 'application/json' } });
		const obj = await env.BUCKET.get(R2_KEY);
		const readBack = obj ? await obj.text() : null;
		const listed = await env.BUCKET.list({ prefix: 'stats/' });
		const count = (listed.objects || []).length;
		return result('BUCKET', readBack === body, `R2 put/get ok, ${count} object(s) under stats/`);
	} catch (e) {
		return fail('BUCKET', e);
	}
}

// --- shared stats (what a future API-Twin app would also read) --------------
async function readStats(env) {
	const stats = { visits: null, events: null, lastRun: null };
	try {
		if (hasMethod(env.KV, 'get')) stats.visits = parseInt((await env.KV.get(KV_VISITS)) || '0', 10) || 0;
	} catch { /* binding absent or store empty */ }
	try {
		if (hasMethod(env.DB, 'first')) {
			const row = await env.DB.first(`SELECT count(*) AS n FROM ${DB_TABLE}`);
			stats.events = row ? row.n : 0;
		}
	} catch { /* table not created yet */ }
	try {
		if (hasMethod(env.BUCKET, 'get')) {
			const obj = await env.BUCKET.get(R2_KEY);
			stats.lastRun = obj ? JSON.parse(await obj.text()) : null;
		}
	} catch { /* object absent */ }
	return stats;
}

// --- small helpers ----------------------------------------------------------
const result = (binding, ok, detail) => ({ name: binding, binding, status: 'ran', ok, detail });
const skip = (binding, detail) => ({ name: binding, binding, status: 'skipped', ok: false, detail });
const fail = (binding, e) => result(binding, false, `error: ${e && e.message ? e.message : e}`);

const json = (obj, status = 200) =>
	new Response(JSON.stringify(obj, null, 2), {
		status,
		headers: { 'content-type': 'application/json; charset=utf-8' }
	});

const html = (body) =>
	new Response(body, { headers: { 'content-type': 'text/html; charset=utf-8', 'cache-control': 'no-store' } });

// The test page. Plain HTML+JS, dark theme (ecosystem default). It calls
// /api/tests on load and renders per-binding status + the shared stats.
const PAGE = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>API-Classic — qubepods</title>
<style>
  :root { color-scheme: dark; }
  body { margin: 0; background: #0b0d10; color: #e6e8eb; font: 15px/1.5 ui-sans-serif, system-ui, -apple-system, Segoe UI, Roboto, sans-serif; }
  main { max-width: 720px; margin: 0 auto; padding: 40px 20px 80px; }
  h1 { font-size: 22px; margin: 0 0 4px; }
  p.sub { color: #9aa4b2; margin: 0 0 28px; }
  code { background: #171a1f; padding: 1px 6px; border-radius: 5px; color: #b9c2cf; }
  .card { background: #12151a; border: 1px solid #1e232b; border-radius: 12px; padding: 16px 18px; margin: 12px 0; }
  .row { display: flex; align-items: center; gap: 12px; }
  .name { font-weight: 600; width: 90px; }
  .pill { font-size: 12px; font-weight: 600; padding: 2px 9px; border-radius: 999px; }
  .ok { background: #10371f; color: #4ade80; }
  .no { background: #3a1417; color: #f87171; }
  .skip { background: #2a2f38; color: #9aa4b2; }
  .detail { color: #9aa4b2; font-size: 13px; margin-top: 6px; }
  .stats { display: grid; grid-template-columns: repeat(3, 1fr); gap: 12px; margin-top: 8px; }
  .stat { background: #12151a; border: 1px solid #1e232b; border-radius: 12px; padding: 14px; text-align: center; }
  .stat .n { font-size: 26px; font-weight: 700; }
  .stat .l { color: #9aa4b2; font-size: 12px; text-transform: uppercase; letter-spacing: .04em; }
  button { background: #2563eb; color: #fff; border: 0; border-radius: 9px; padding: 9px 16px; font-weight: 600; cursor: pointer; }
  button:hover { background: #1d4ed8; }
  h2 { font-size: 13px; text-transform: uppercase; letter-spacing: .05em; color: #9aa4b2; margin: 28px 0 8px; }
</style>
</head>
<body>
<main>
  <h1>API-Classic</h1>
  <p class="sub">A classic HTTP API worker — <b>KV</b>, <b>SQLite</b>, and <b>R2</b> auto-bound from the <code>qube.json5</code> manifest. <code>qube run</code> / <code>qube deploy</code>, no wrangler.</p>

  <h2>Shared stats</h2>
  <div class="stats">
    <div class="stat"><div class="n" id="s-visits">–</div><div class="l">KV visits</div></div>
    <div class="stat"><div class="n" id="s-events">–</div><div class="l">DB events</div></div>
    <div class="stat"><div class="n" id="s-last">–</div><div class="l">R2 last run</div></div>
  </div>

  <h2>Binding tests</h2>
  <div id="tests"></div>

  <p style="margin-top:20px"><button id="run">Run tests again</button></p>
</main>
<script>
  const pill = (r) => r.status === 'skipped'
    ? '<span class="pill skip">skipped</span>'
    : r.ok ? '<span class="pill ok">pass</span>' : '<span class="pill no">fail</span>';

  async function run() {
    const el = document.getElementById('tests');
    el.innerHTML = '<div class="card">running…</div>';
    const res = await fetch('/api/tests').then((r) => r.json()).catch((e) => ({ error: String(e) }));
    if (res.error) { el.innerHTML = '<div class="card">error: ' + res.error + '</div>'; return; }
    el.innerHTML = res.results.map((r) =>
      '<div class="card"><div class="row"><span class="name">env.' + r.binding + '</span>' + pill(r) + '</div>' +
      '<div class="detail">' + (r.detail || '') + '</div></div>'
    ).join('');
    const s = res.stats || {};
    document.getElementById('s-visits').textContent = s.visits ?? '–';
    document.getElementById('s-events').textContent = s.events ?? '–';
    document.getElementById('s-last').textContent = s.lastRun ? new Date(s.lastRun.at).toLocaleTimeString() : '–';
  }
  document.getElementById('run').addEventListener('click', run);
  run();
</script>
</body>
</html>`;
