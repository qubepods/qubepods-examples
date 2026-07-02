// HTMX-Booking — a hypermedia app on qubepods: the server renders HTML
// fragments, htmx swaps them into the page. No build step, no frontend
// framework, no client-side state — the DOM is the state, the server is the
// source of truth.
//
// The point of this example: htmx has NO widgets of its own. It is ~14 kB of
// attributes (hx-get, hx-post, hx-target, hx-trigger, hx-swap) that let plain
// HTML talk to this worker. The "date picker" here is the browser's native
// <input type="date"> — built into every browser down to iPad Safari (the
// ecosystem's compatibility floor), zero JS. Everything dynamic on the page is
// a fragment rendered by the handlers below.
//
// Three htmx mechanisms are demonstrated, each marked with a `[htmx]` comment:
//   1. attribute-driven requests — the date input GETs /slots on change;
//   2. out-of-band swaps (hx-swap-oob) — /book and /bookings/:id responses
//      carry a <span id="count"> that updates the header badge wherever it is;
//   3. server-sent events via the HX-Trigger response header — a delete tells
//      the slots panel to refresh without knowing where it lives;
//   4. polling — the server clock re-fetches itself every second
//      (hx-trigger="every 1s"), so what you see ticking is the SERVER's time
//      and the edge colo that rendered the fragment.
//
// Persistence is the project's light-fast SQLite (env.DB), injected from the
// qube.json5 imports — same reserved-binding story as api-classic.

const SLOTS = ['09:00', '10:00', '11:00', '12:00', '13:00', '14:00', '15:00', '16:00', '17:00'];

export default {
	async fetch(request, env) {
		const url = new URL(request.url);
		try {
			await ensureTable(env);
			if (request.method === 'GET' && url.pathname === '/') return html(await page(env, request));
			if (request.method === 'GET' && url.pathname === '/clock') return html(clockFragment(request));
			if (request.method === 'GET' && url.pathname === '/slots')
				return html(await slotsFragment(env, url.searchParams.get('date')));
			if (request.method === 'GET' && url.pathname === '/bookings') return html(await bookingsFragment(env));
			if (request.method === 'POST' && url.pathname === '/book') return book(env, request);
			if (request.method === 'DELETE' && url.pathname.startsWith('/bookings/'))
				return unbook(env, url.pathname.slice('/bookings/'.length));
			return html('<p>not found</p>', 404);
		} catch (e) {
			return html(`<p class="notice">error: ${esc(String(e && e.message ? e.message : e))}</p>`, 500);
		}
	}
};

// --- handlers -----------------------------------------------------------------

// POST /book (form: date, slot, guest). Always 200 with the re-rendered slots
// panel — if the slot was taken in a race, the fresh render shows it booked and
// carries a notice, which is more honest than a client-side error state. (htmx
// does not swap non-2xx responses by default, so fragments ride 200s.)
async function book(env, request) {
	const form = await request.formData();
	const day = validDate(form.get('date'));
	const slot = String(form.get('slot') || '');
	// No name typed? Sign the booking with the edge location it came from —
	// the same gate-forwarded info the server clock shows.
	const guest = String(form.get('guest') || '').trim().slice(0, 40) || edgeName(request);
	if (!day || !SLOTS.includes(slot)) return html('<p class="notice">pick a date and a slot</p>');

	let notice = '';
	try {
		await env.DB.exec('INSERT INTO bookings (day, slot, guest, at) VALUES (?, ?, ?, ?)', day, slot, guest, Date.now());
	} catch {
		notice = `<p class="notice">${slot} on ${day} was just taken — pick another slot</p>`; // UNIQUE(day, slot)
	}

	// [htmx] The main body swaps into the request's hx-target (#slots); the
	// hx-swap-oob span updates the header badge OUT OF BAND — one response,
	// two regions, no client code. The HX-Trigger header fires a DOM event the
	// bookings list subscribes to (hx-trigger="bookings-changed from:body").
	const body = notice + (await slotsFragment(env, day)) + (await countOob(env));
	return html(body, 200, { 'HX-Trigger': 'bookings-changed' });
}

// DELETE /bookings/:id. The <li> is the hx-target with hx-swap="outerHTML", so
// the non-OOB part of the response (nothing) replaces it — i.e. removes it.
// The OOB span still updates the badge, and HX-Trigger refreshes the slots
// panel in case the freed slot's date is on screen.
async function unbook(env, id) {
	await env.DB.exec('DELETE FROM bookings WHERE id = ?', Number(id) || 0);
	return html(await countOob(env), 200, { 'HX-Trigger': 'slots-changed' });
}

// --- fragments ------------------------------------------------------------------
// Each of these is a complete, ready-to-insert piece of HTML. This is the whole
// htmx contract: the wire format IS the UI. (The twin does the same thing over a
// WebSocket; this is the request/response version.)

async function slotsFragment(env, date) {
	const day = validDate(date);
	if (!day) return '<p class="notice">pick a date</p>';
	const taken = new Map((await rows(env, 'SELECT slot, guest FROM bookings WHERE day = ?', day)).map((r) => [r.slot, r.guest]));
	const buttons = SLOTS.map((slot) =>
		taken.has(slot)
			? `<button class="slot booked" disabled>${slot} · ${esc(taken.get(slot))}</button>`
			: // [htmx] hx-vals pins the slot; hx-include pulls the guest + date
				// inputs from elsewhere on the page into the POST body.
				`<button class="slot" hx-post="/book" hx-vals='{"slot":"${slot}"}' hx-include="#guest,#date" hx-target="#slots">${slot}</button>`
	).join('');
	return `<p class="sub">free slots on <b>${day}</b> — click one to book it</p><div class="grid">${buttons}</div>`;
}

// GET /clock — a server clock with the edge location that rendered it. The page
// polls this every second ([htmx] hx-trigger="every 1s"), so each tick is a
// fresh server render — no client-side Date, no JS timer.
//
// Where the location comes from: a qubepods user worker is a Workers-for-
// Platforms dispatch target running in "untrusted mode", which CANNOT read
// `request.cf` — the gate holds the real edge request and forwards geo/colo as
// `x-qube-cf-*` headers. Read those first; fall back to `request.cf` for paths
// that do have it (e.g. `qube run`'s direct preview).
function clockFragment(request) {
	const { colo, country, city, ray } = edgeInfo(request);
	const now = new Date();
	return (
		`<div class="time">${now.toISOString().slice(11, 19)} <small>UTC</small></div>` +
		`<div class="edge">rendered at edge <b>${esc(colo || '?')}</b> · ${esc(city ? `${city}, ${country}` : country || '?')} · ray ${esc(ray || '?')}</div>`
	);
}

// The edge location of a request: gate-forwarded x-qube-cf-* headers first
// (a WfP user worker can't read request.cf), request.cf as the fallback.
function edgeInfo(request) {
	const cf = request.cf || {};
	const h = request.headers;
	return {
		colo: h.get('x-qube-cf-colo') || cf.colo || '',
		country: h.get('x-qube-cf-country') || cf.country || '',
		city: h.get('x-qube-cf-city') || cf.city || '',
		ray: h.get('cf-ray') || ''
	};
}

// A human-readable stand-in name: "Frankfurt (FRA)", or the colo/country alone.
function edgeName(request) {
	const e = edgeInfo(request);
	return (e.city ? `${e.city} (${e.colo || e.country})` : e.colo || e.country) || 'guest';
}

async function bookingsFragment(env) {
	const list = await rows(env, 'SELECT id, day, slot, guest FROM bookings ORDER BY day, slot');
	if (!list.length) return '<li class="empty">no bookings yet</li>';
	return list.map((b) =>
		`<li><span><b>${b.day}</b> ${b.slot} — ${esc(b.guest)}</span>` +
		// [htmx] delete-in-place: target the row itself, swap its outerHTML with
		// the (empty) response body — the row disappears, no list re-render.
		`<button class="x" hx-delete="/bookings/${b.id}" hx-target="closest li" hx-swap="outerHTML" title="cancel">✕</button></li>`
	).join('');
}

async function countOob(env) {
	const row = await env.DB.first('SELECT count(*) AS n FROM bookings');
	return `<span id="count" hx-swap-oob="true">${row ? row.n : 0}</span>`;
}

// --- helpers --------------------------------------------------------------------

async function ensureTable(env) {
	await env.DB.exec(
		'CREATE TABLE IF NOT EXISTS bookings (id INTEGER PRIMARY KEY AUTOINCREMENT, day TEXT NOT NULL, slot TEXT NOT NULL, guest TEXT NOT NULL, at INTEGER NOT NULL, UNIQUE(day, slot))'
	);
}

// The SQL gateway's query result, defensively unwrapped (array or { rows }).
async function rows(env, sql, ...params) {
	const res = await env.DB.query(sql, ...params);
	return Array.isArray(res) ? res : (res && res.rows) || [];
}

const validDate = (s) => (/^\d{4}-\d{2}-\d{2}$/.test(String(s || '')) ? String(s) : null);

// Fragments are HTML — anything user-typed (the guest name) MUST be escaped
// before it rides the wire. This is the one discipline hypermedia apps keep.
const esc = (s) => String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);

const html = (body, status = 200, headers = {}) =>
	new Response(body, {
		status,
		headers: { 'content-type': 'text/html; charset=utf-8', 'cache-control': 'no-store', ...headers }
	});

// --- the page ---------------------------------------------------------------------
// The only full document. Note what is ABSENT: no fetch(), no JSON parsing, no
// render function, no state variable. htmx (pinned + SRI from the official CDN
// snippet) reads the hx-* attributes; the worker above does the rest.
async function page(env, request) {
	const today = new Date().toISOString().slice(0, 10);
	const row = await env.DB.first('SELECT count(*) AS n FROM bookings');
	const placeholder = esc(edgeName(request));
	return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>HTMX-Booking — qubepods</title>
<script src="https://cdn.jsdelivr.net/npm/htmx.org@2.0.4/dist/htmx.min.js" integrity="sha384-HGfztofotfshcF7+8n44JQL2oJmowVChPTg48S+jvZoztPfvwD79OC/LTtG6dMp+" crossorigin="anonymous"></script>
<style>
  :root { color-scheme: dark; }
  body { margin: 0; background: #0b0d10; color: #e6e8eb; font: 15px/1.5 ui-sans-serif, system-ui, -apple-system, Segoe UI, Roboto, sans-serif; }
  main { max-width: 720px; margin: 0 auto; padding: 40px 20px 80px; }
  h1 { font-size: 22px; margin: 0 0 4px; }
  h1 #count { background: #10371f; color: #4ade80; font-size: 13px; padding: 2px 10px; border-radius: 999px; vertical-align: 3px; margin-left: 8px; }
  p.sub { color: #9aa4b2; margin: 0 0 18px; }
  code { background: #171a1f; padding: 1px 6px; border-radius: 5px; color: #b9c2cf; }
  h2 { font-size: 13px; text-transform: uppercase; letter-spacing: .05em; color: #9aa4b2; margin: 28px 0 8px; }
  .card { background: #12151a; border: 1px solid #1e232b; border-radius: 12px; padding: 16px 18px; margin: 12px 0; }
  .row { display: flex; gap: 12px; flex-wrap: wrap; }
  label { display: block; color: #9aa4b2; font-size: 12px; text-transform: uppercase; letter-spacing: .04em; margin-bottom: 4px; }
  input { background: #171a1f; color: #e6e8eb; border: 1px solid #262c36; border-radius: 9px; padding: 8px 12px; font: inherit; color-scheme: dark; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(110px, 1fr)); gap: 10px; }
  .slot { background: #2563eb; color: #fff; border: 0; border-radius: 9px; padding: 10px 8px; font-weight: 600; cursor: pointer; }
  .slot:hover { background: #1d4ed8; }
  .slot.booked { background: #171a1f; color: #5c6672; cursor: default; }
  .notice { background: #3a1417; color: #f87171; border-radius: 9px; padding: 8px 12px; font-size: 13px; }
  ul { list-style: none; margin: 0; padding: 0; }
  li { display: flex; align-items: center; justify-content: space-between; border-bottom: 1px solid #1e232b; padding: 9px 2px; }
  li.empty { color: #5c6672; border: 0; }
  .x { background: none; border: 0; color: #f87171; cursor: pointer; font-size: 14px; }
  .clock .time { font-size: 28px; font-weight: 700; font-variant-numeric: tabular-nums; }
  .clock .time small { font-size: 13px; color: #9aa4b2; font-weight: 400; }
  .clock .edge { color: #9aa4b2; font-size: 13px; margin-top: 2px; }
  button.htmx-request { opacity: .5; transition: opacity .15s; }
</style>
</head>
<body>
<main>
  <h1>HTMX-Booking<span id="count">${row ? row.n : 0}</span></h1>
  <p class="sub">A hypermedia app: the worker renders <b>HTML fragments</b>, htmx swaps them in. The date picker is the browser's own <code>&lt;input type="date"&gt;</code> — htmx ships no widgets, and needs none.</p>

  <div class="card">
    <div class="row">
      <div><label for="guest">your name</label><input id="guest" name="guest" placeholder="${placeholder}" maxlength="40" /></div>
      <!-- [htmx] the whole "app wiring" for the date picker is these three
           attributes: on change, GET /slots?date=…, put the response in #slots. -->
      <div><label for="date">date</label><input type="date" id="date" name="date" value="${today}" min="${today}"
        hx-get="/slots" hx-target="#slots" hx-trigger="change" /></div>
    </div>
  </div>

  <h2>Server clock</h2>
  <!-- [htmx] polling: re-fetch the fragment every second. The ticking you see
       is server renders arriving, not a client-side timer. -->
  <div id="clock" class="card clock" hx-get="/clock" hx-trigger="load, every 1s"></div>

  <h2>Slots</h2>
  <!-- [htmx] loads itself on page load, re-loads whenever anything on the page
       fires slots-changed (the delete handler does, via HX-Trigger), and polls
       every 3s so a booking made in ANOTHER browser shows up here — the panels
       converge on the server's state without a socket. (Push instead of poll is
       the twin's job: the same fragments fanned over its WebSocket.) -->
  <div id="slots" class="card" hx-get="/slots" hx-include="#date" hx-trigger="load, every 3s, slots-changed from:body"></div>

  <h2>Bookings</h2>
  <ul id="bookings" class="card" hx-get="/bookings" hx-trigger="load, every 3s, bookings-changed from:body"></ul>
</main>
</body>
</html>`;
}
