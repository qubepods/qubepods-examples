# htmx-booking

A slot-booking app built the **hypermedia** way: the worker renders **HTML
fragments**, and [htmx](https://htmx.org) swaps them into the page. There is no
build step, no frontend framework, no client-side state — the DOM is the state,
the server is the source of truth.

Pick a date, click a free slot to book it, cancel from the list. Bookings live
in the project's **SQLite database** (`env.DB`, light-fast tier), auto-bound
from the manifest like every store in these examples — no wrangler, no ids.

## Why this example exists

`api-classic` shows the classic split: the worker returns **JSON**, and a page
script fetches, parses, and re-renders. This example is the other answer to the
same question: the worker returns **the UI itself**, one fragment per
interaction, and the page carries no app code at all — just one pinned
`<script>` (htmx, ~14 kB gz) reading `hx-*` attributes.

It is also the request/response cousin of **twin-counter**: a twin fans
server-rendered diffs to every subscriber over a WebSocket; htmx pulls
server-rendered fragments over HTTP. Same philosophy — thin client,
server-authoritative — different wire.

## "Which widgets does htmx have?" — none, and that's the point

htmx ships **zero widgets**. It is not a component library; it is a set of HTML
attributes (`hx-get`, `hx-post`, `hx-target`, `hx-trigger`, `hx-swap`) that let
any element make requests and swap responses. Widgets come from:

- **the browser** — the date picker here is the native `<input type="date">`
  (works everywhere down to iPad Safari, zero JS); `<datalist>`, `<dialog>`,
  `<details>`, `type="range"`/`"color"` cover a lot more than people remember;
- **the server** — the slot grid and booking list are "widgets" only in the
  sense that this worker renders them as HTML;
- **standalone JS libraries** when the native control isn't enough (e.g.
  flatpickr for a fancier calendar) — htmx coexists with them via
  `hx-preserve`.

What htmx's docs call [examples](https://htmx.org/examples/) — active search,
click-to-edit, infinite scroll, modals — are **patterns** you compose from
those attributes, not components you import.

## The three htmx mechanisms on display

1. **Attribute-driven requests** — the entire "app wiring" for the date picker:

   ```html
   <input type="date" name="date" hx-get="/slots" hx-target="#slots" hx-trigger="change" />
   ```

2. **Out-of-band swaps** — booking and cancelling both return a
   `<span id="count" hx-swap-oob="true">` alongside the main fragment: one
   response updates two page regions, no client code.

3. **`HX-Trigger` response headers** — a cancel fires `slots-changed`; the
   slots panel subscribes with `hx-trigger="slots-changed from:body"` and
   refreshes itself. The server signals *what happened*, not *where to poke*.

4. **Polling** — the server clock re-fetches itself with
   `hx-trigger="every 1s"`. The ticking you see is server renders arriving,
   not a client-side timer: each fragment carries the server's UTC time and
   the **edge colo** that rendered it. The slots and bookings panels poll too
   (`every 3s`), so a booking made in **another browser** shows up in yours —
   the panels converge on the server's state without a socket. (Push instead
   of poll is the twin's job: the same fragments fanned over its WebSocket —
   see `twin-counter`.)

## The edge knows who you are (qubepods-specific)

The clock — and the default guest name — come from the request's edge
location. One platform detail matters here: a qubepods user worker is a
Workers-for-Platforms dispatch target running in *untrusted mode*, which
**cannot read `request.cf`**. The gate holds the real edge request and
forwards geo/colo as **`x-qube-cf-*` headers**; the worker reads those first
and falls back to `request.cf` (which `qube run`'s direct preview does have).

Book a slot without typing a name and the booking is signed by where it came
from — e.g. `Frankfurt (FRA)` — the same info the clock shows.

One discipline hypermedia apps keep: fragments are HTML, so anything user-typed
(the guest name) is **escaped server-side** before it rides the wire.

## Run it

Needs a project with the **Backend** switch on (it's a classic worker qube,
like `api-classic`). From the example's folder in Qubonaut
(`app.qubepods.com`) or a terminal with a project token:

```sh
qube run       # live preview against the project's real SQLite
qube deploy    # ship it to your project's *.qubepod.app URL
```

Live at [electric-moss-27.qubepod.app](https://electric-moss-27.qubepod.app/)
(deployed in a project named `edge` — set `project` in `qube.json5` to yours).
