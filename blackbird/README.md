# blackbird — a static-asset qube (chess in the browser)

A full chess game you play in the browser — board, rules, and engine — shipped
as **static files**. There is no q64 source to compile and no backend: the whole
qube is the `web/` folder, served as-is. It's the example for bringing a
hand-written web app (HTML/CSS/JS + whatever assets it loads) to qubepods
**unchanged**.

## What it shows

- **A static-site qube.** `qube.json5` declares `static: { dir: "web" }` and
  nothing else — no `entry`, no `component`. `qube deploy` zips `web/` and serves
  it at the edge. The page's own `index.html` is the page.
- **An asset wasm, not a q64 build.** The Blackbird engine
  (`web/engine/blackbird.wasm`) is just a file the page fetches in a Web Worker —
  one static asset among the images and scripts, **not** the pod's component. A
  static qube has no wasm to run; the browser runs this one.
- **No compile step.** Everything needed to play is committed (engine ~130 KB +
  the board UI), so a plain checkout runs, and a deploy ships the same bytes.

## Run it locally

The page fetches wasm, so it needs an HTTP server (not `file://`):

```sh
python3 -m http.server -d web 8080
# open http://localhost:8080
```

You play white; the engine answers automatically. **Engine move** plays for
whichever side is to move, so you can take black or watch it play itself.

## Deploy it

Open your project in **Qubonaut** (`app.qubepods.com`), clone this repo in its
terminal, and from this folder:

```sh
qube deploy
```

That ships `web/` as the project's static site and prints the live URL. No
Backend switch, no build — it's files.

## Layout

```
blackbird/
├── qube.json5     # static-site manifest — `static: { dir: "web" }`
└── web/           # the whole deploy: served as-is
    ├── index.html # the page
    ├── app.js     # board UI: render + clicks
    ├── worker.js  # runs the engine off the main thread
    ├── style.css
    ├── engine/
    │   └── blackbird.wasm   # the chess engine — an ASSET the page loads
    └── img/       # piece sprites
```

## How it works

`index.html` / `app.js` render the board and capture clicks; they hand moves to
`worker.js`, which runs `engine/blackbird.wasm` and answers with the engine's
move. The engine is the single source of truth for the rules — after every move
it reports the full state (legal moves, check/mate), so the UI never
re-implements chess. All of that is plain browser code; qubepods just serves the
files.
