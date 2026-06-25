# qube-rocks — Asteroids-lite in the quine engine (single-player, static)

A small **Asteroids** you fly in the browser: steer a ship, thrust, fire, and
break rocks into smaller rocks. **Single-player, client-only, no backend, no
server** — the whole game runs in the browser.

It's a **static-asset qube**, like [`blackbird`](../blackbird/): `qube deploy`
serves the `web/` folder as-is and **the page is its own host**. The difference
from Blackbird is that Qube Rocks doesn't bundle its own wasm — its harness loads
the **shared quine 3D engine** from `cdn.qubeworlds.com` and feeds it this qube's
scene + skill through the engine's generic inject API. So the qube ships only its
*content* (scene + skill) and a thin harness; the engine is the shared CDN one.

## What it shows

| Feature | Where | What it does |
|---|---|---|
| **64 Hz fixed game loop** | `web/scene.json` → `"fixedHz": 64` | The sim ticks at 64 Hz independent of render rate (a `Time::<Fixed>::from_hz(64.0)` equivalent). |
| **Render interpolation** | `web/scene.json` → `"interpolate": true` | The renderer lerps between sim ticks by the accumulator overstep, so 64 Hz sim on a 60 Hz screen stays smooth. |
| **Steering** | skill → `transform.rotation` | The ship turns to face its heading from the controller's turn axis. |
| **Spawn / despawn** | skill → `world.spawn` / `world.despawn` | Bullets and rock fragments are created and removed at runtime. |
| **Cone mesh** | `web/scene.json` → `"kind": "cone"` | A built-in directional primitive for the ship (apex = forward). |
| **Reusable controller** | `web/scene.json` → `"overlay"` | The shared [`controls.js`](https://cdn.qubeworlds.com/overlays/controls.js) overlay maps touch + keys onto the input axes. |

## Controls

The linked `controls.js` overlay renders on-screen buttons (and mirrors the
keyboard), driving the engine's input axes that the skill reads via `input(axis)`:

| Action | Axis | Touch | Keyboard |
|---|---|---|---|
| Turn left / right | 0 | ◄ ► | ← → / A D |
| Thrust | 1 | ▲ | ↑ / W |
| Fire | 2 | ● | Space |

## How it maps to a Bevy game

| Bevy | Qube Rocks |
|---|---|
| `App::new().add_plugins(DefaultPlugins)` | the harness boots the quine engine from the CDN |
| `insert_resource(Time::<Fixed>::from_hz(64.0))` | `scene.json` → `"fixedHz": 64` |
| `add_systems(Startup, setup)` | `scene.json` → `entities` |
| `add_systems(FixedUpdate, simulate_game)` | `qube-rocks.skill.js` → `onPreStep(dt)` |
| `add_systems(Update, …)` (input/UI per frame) | the `controls.js` overlay |

## Single-player vs multiplayer

This is the **single-player / client-only** tier: the deterministic game loop runs
in the browser (the skill), so there's nothing authoritative to hold and static
Cloudflare hosting is the right fit. A **multiplayer** Qube Rocks would be a
*different shape* — the [`@world/qubegame`](https://github.com/qubeworlds/world/tree/main/packages/qubegame)
client (`mountQuine`) joining a **game-server room**, with the authoritative sim
running in the gameserver on a **dedicated node (not Cloudflare)**. Same engine,
same scene, same controller — only *who holds authority* and *the transport*
change. The 64 Hz deterministic fixed-step here is exactly what that server-
authoritative model wants, so it ports cleanly.

## Files

- `qube.json5` — `static: { dir: "web" }`. No q64 compile, no component, no backend.
- `web/index.html` — the harness (the host): loads the quine engine from the CDN, injects `scene.json` + the skill, mounts the controller overlay, and wires `host.input`.
- `web/scene.json` — the entities, the `fixedHz`/`interpolate` knobs, and the links to the skill + the controller overlay.
- `web/qube-rocks.skill.js` — the game loop (deterministic; the engine runs it as the scene's skill).

## Run it

In Qubonaut (`app.qubepods.com`), clone this repo and from this folder:

```sh
qube run        # serves web/ — open the Preview
```

Because the harness is self-contained, it runs in **any** static host (Qubonaut
Preview, `qube deploy`, or a plain web server) — no special engine/overlay host
seam needed. You can also open `web/index.html` over any static server locally.

> The page reaches `cdn.qubeworlds.com` for the engine bundle + the `controls.js`
> overlay. If that's blocked (offline), the page shows a load error instead of the
> game.
