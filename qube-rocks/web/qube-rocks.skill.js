// qube-rocks.skill.js — Asteroids-lite, the FixedUpdate of Qube Rocks.
//
// Runs in the quine engine's QuickJS sandbox against the prelude facade, one
// `onPreStep(dt)` per fixed tick (the scene asks for 64 Hz). It reads the three
// input axes the linked `controls.js` overlay drives — turn (0), thrust (1),
// fire (2) — steers the ship with the transform.rotation native, and spawns /
// despawns bullets and rock fragments with world.spawn / world.despawn. No
// backend, no host RNG: a tiny seeded LCG keeps it deterministic (same as the
// engine's core), so a replay of the same inputs reproduces the same game.
(function () {
  var TURN = 3.0, // rad/s
    THRUST = 7.0, // units/s^2
    DRAG = 0.5, // velocity damping per second
    MAX_SPEED = 6.0,
    BULLET_SPEED = 9.0,
    BULLET_LIFE = 1.1, // seconds
    FIRE_COOLDOWN = 0.18,
    W = 7.0, // half play-field (x and z)
    SHIP_R = 0.32,
    BULLET_R = 0.08,
    NUM_ROCKS = 5;

  // Deterministic pseudo-random in [0,1): a seeded LCG, NOT Math.random (which
  // isn't guaranteed reproducible). Keeps the game replayable.
  var seed = 0x1234567;
  function rnd() {
    seed = (seed * 1103515245 + 12345) & 0x7fffffff;
    return seed / 0x7fffffff;
  }
  function wrap(v) {
    if (v > W) return v - 2 * W;
    if (v < -W) return v + 2 * W;
    return v;
  }

  var ship = world.get("ship");
  var nose = world.get("nose");
  var angle = 0; // heading on the X-Z plane (radians)
  var vx = 0, vz = 0;
  var cooldown = 0;
  var rocks = []; // { e, x, z, vx, vz, r, big }
  var bullets = []; // { e, x, z, vx, vz, life }
  var started = false;

  function dir() {
    return { x: Math.sin(angle), z: Math.cos(angle) };
  }

  function spawnRock(x, z, big) {
    var e = world.spawn(big ? "rockBig" : "rockSmall");
    if (!e) return;
    var sp = big ? 1.2 : 2.0;
    rocks.push({ e: e, x: x, z: z, vx: (rnd() - 0.5) * sp, vz: (rnd() - 0.5) * sp, r: big ? 0.7 : 0.35, big: big });
    e.transform.position = { x: x, y: 0, z: z };
  }

  function resetShip() {
    var p = ship.transform.position;
    ship.transform.position = { x: 0, y: p.y, z: 0 };
    vx = 0;
    vz = 0;
    angle = 0;
  }

  onPreStep(function (dt) {
    // First tick: scatter the rocks around the edges (away from the ship).
    if (!started) {
      started = true;
      for (var k = 0; k < NUM_ROCKS; k++) {
        var ex = (rnd() - 0.5) * 2 * W, ez = (rnd() - 0.5) * 2 * W;
        if (Math.abs(ex) < 2 && Math.abs(ez) < 2) ex += 3; // not on top of the ship
        spawnRock(ex, ez, true);
      }
    }

    // --- steer + thrust (the controller's axes) ---
    angle += input(0) * TURN * dt; // turn: ◄ ►
    var d = dir();
    var th = input(1); // thrust: ▲
    vx += d.x * THRUST * th * dt;
    vz += d.z * THRUST * th * dt;
    var damp = Math.max(0, 1 - DRAG * dt);
    vx *= damp;
    vz *= damp;
    var sp = Math.sqrt(vx * vx + vz * vz);
    if (sp > MAX_SPEED) {
      vx = (vx / sp) * MAX_SPEED;
      vz = (vz / sp) * MAX_SPEED;
    }

    var p = ship.transform.position;
    var sx = wrap(p.x + vx * dt), sz = wrap(p.z + vz * dt);
    ship.transform.position = { x: sx, y: 0, z: sz };
    ship.transform.rotation = { x: 0, y: angle, z: 0 }; // steer the mesh too
    // The bright nose marks heading (a sphere's spin is invisible) and is where
    // bullets leave from.
    nose.transform.position = { x: sx + d.x * 0.5, y: 0, z: sz + d.z * 0.5 };

    // --- fire (axis 2), rate-limited ---
    cooldown -= dt;
    if (input(2) > 0.5 && cooldown <= 0) {
      cooldown = FIRE_COOLDOWN;
      var b = world.spawn("bullet");
      if (b) {
        var bx = sx + d.x * 0.5, bz = sz + d.z * 0.5;
        bullets.push({ e: b, x: bx, z: bz, vx: d.x * BULLET_SPEED + vx, vz: d.z * BULLET_SPEED + vz, life: BULLET_LIFE });
        b.transform.position = { x: bx, y: 0, z: bz };
      }
    }

    // --- advance rocks ---
    for (var i = 0; i < rocks.length; i++) {
      var r = rocks[i];
      r.x = wrap(r.x + r.vx * dt);
      r.z = wrap(r.z + r.vz * dt);
      r.e.transform.position = { x: r.x, y: 0, z: r.z };
      // Ship collision -> respawn the ship at the centre.
      var ddx = r.x - sx, ddz = r.z - sz;
      if (ddx * ddx + ddz * ddz < (r.r + SHIP_R) * (r.r + SHIP_R)) resetShip();
    }

    // --- advance bullets, expire, and test against rocks ---
    for (var j = bullets.length - 1; j >= 0; j--) {
      var bl = bullets[j];
      bl.x = wrap(bl.x + bl.vx * dt);
      bl.z = wrap(bl.z + bl.vz * dt);
      bl.life -= dt;
      bl.e.transform.position = { x: bl.x, y: 0, z: bl.z };

      var hit = -1;
      for (var m = 0; m < rocks.length; m++) {
        var rk = rocks[m];
        var hx = rk.x - bl.x, hz = rk.z - bl.z;
        if (hx * hx + hz * hz < (rk.r + BULLET_R) * (rk.r + BULLET_R)) { hit = m; break; }
      }
      if (hit >= 0) {
        var rock = rocks[hit];
        world.despawn(rock.e);
        rocks.splice(hit, 1);
        if (rock.big) {
          // Split into two smaller rocks flying apart.
          spawnRock(rock.x, rock.z, false);
          spawnRock(rock.x, rock.z, false);
        }
        world.despawn(bl.e);
        bullets.splice(j, 1);
        continue;
      }
      if (bl.life <= 0) {
        world.despawn(bl.e);
        bullets.splice(j, 1);
      }
    }

    // Cleared the field? Re-seed a fresh wave.
    if (rocks.length === 0) started = false;
  });
})();
