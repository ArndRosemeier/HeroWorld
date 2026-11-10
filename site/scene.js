// The cover's live scene: a made-up HeroWorld town at golden hour, drawn
// like a comic panel (flat colour bands, ink outlines, halftone dots in the
// shade). Plain WebGL 2, no libraries, no textures: everything is built
// here from boxes, prisms and lathed shapes, and windows and streets are
// drawn by the shader from coordinates.
//
// What moves: the camera circling the Aether Spire (leaning with the
// mouse), cars on the streets, flying taxis, the airship on its round, a
// hero streaking over the roofs, the Tesla tower's arcs in the villain
// quarter, steam from the atomic plant and a rocket that lifts off now
// and then. Same layout every visit (a fixed seed).
//
// Usage: HWScene.start(canvas) -> false when WebGL 2 is missing (the page
// keeps its still picture then).
(function () {
  "use strict";

  // ---------------------------------------------------------------- maths
  function rng(seed) {
    var s = seed >>> 0;
    return function () {
      s = (s + 0x6d2b79f5) >>> 0;
      var t = s;
      t = Math.imul(t ^ (t >>> 15), t | 1);
      t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
      return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
  }
  function perspective(fovy, aspect, near, far) {
    var f = 1 / Math.tan(fovy / 2), nf = 1 / (near - far);
    return [f / aspect, 0, 0, 0, 0, f, 0, 0, 0, 0, (far + near) * nf, -1, 0, 0, 2 * far * near * nf, 0];
  }
  function lookAt(e, c, up) {
    var zx = e[0] - c[0], zy = e[1] - c[1], zz = e[2] - c[2];
    var zl = Math.hypot(zx, zy, zz); zx /= zl; zy /= zl; zz /= zl;
    var xx = up[1] * zz - up[2] * zy, xy = up[2] * zx - up[0] * zz, xz = up[0] * zy - up[1] * zx;
    var xl = Math.hypot(xx, xy, xz); xx /= xl; xy /= xl; xz /= xl;
    var yx = zy * xz - zz * xy, yy = zz * xx - zx * xz, yz = zx * xy - zy * xx;
    return [xx, yx, zx, 0, xy, yy, zy, 0, xz, yz, zz, 0,
      -(xx * e[0] + xy * e[1] + xz * e[2]), -(yx * e[0] + yy * e[1] + yz * e[2]), -(zx * e[0] + zy * e[1] + zz * e[2]), 1];
  }
  function mul(a, b) {
    var o = new Array(16);
    for (var c = 0; c < 4; c++) for (var r = 0; r < 4; r++) {
      o[c * 4 + r] = a[r] * b[c * 4] + a[4 + r] * b[c * 4 + 1] + a[8 + r] * b[c * 4 + 2] + a[12 + r] * b[c * 4 + 3];
    }
    return o;
  }
  function hex(h) {
    return [parseInt(h.slice(1, 3), 16) / 255, parseInt(h.slice(3, 5), 16) / 255, parseInt(h.slice(5, 7), 16) / 255];
  }

  // ---------------------------------------------------------- mesh builder
  // Per vertex: position 3, normal 3, outline direction 3, colour 3,
  // u v kind 3 (kind: 0 plain, 1 windows, 2 glow, 3 villain windows,
  // 4 ground, 5 glass).
  var STRIDE = 15;
  function Mesh() { this.v = []; }
  Mesh.prototype.vert = function (p, n, o, c, u, v, k) {
    this.v.push(p[0], p[1], p[2], n[0], n[1], n[2], o[0], o[1], o[2], c[0], c[1], c[2], u, v, k);
  };
  // A triangle with a flat normal; `centre` gives the outline direction.
  Mesh.prototype.tri = function (a, b, c, col, centre, uva, uvb, uvc, k) {
    var ux = b[0] - a[0], uy = b[1] - a[1], uz = b[2] - a[2];
    var vx = c[0] - a[0], vy = c[1] - a[1], vz = c[2] - a[2];
    var n = [uy * vz - uz * vy, uz * vx - ux * vz, ux * vy - uy * vx];
    var l = Math.hypot(n[0], n[1], n[2]) || 1; n = [n[0] / l, n[1] / l, n[2] / l];
    var self = this;
    [[a, uva], [b, uvb], [c, uvc]].forEach(function (pv) {
      var p = pv[0], d = [p[0] - centre[0], p[1] - centre[1], p[2] - centre[2]];
      var dl = Math.hypot(d[0], d[1], d[2]) || 1;
      self.vert(p, n, [d[0] / dl, d[1] / dl, d[2] / dl], col, pv[1][0], pv[1][1], k);
    });
  };
  Mesh.prototype.quad = function (a, b, c, d, col, centre, w, h, k) {
    // a b c d counter-clockwise seen from outside; uv in metres.
    this.tri(a, b, c, col, centre, [0, 0], [w, 0], [w, h], k);
    this.tri(a, c, d, col, centre, [0, 0], [w, h], [0, h], k);
  };
  // A box from (x0,y0,z0) to (x1,y1,z1); walls take `kind`, the top `topKind`.
  Mesh.prototype.box = function (x0, y0, z0, x1, y1, z1, col, kind, top, topKind) {
    var c = [(x0 + x1) / 2, (y0 + y1) / 2, (z0 + z1) / 2];
    var w = x1 - x0, d = z1 - z0, h = y1 - y0;
    this.quad([x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1], col, c, w, h, kind);
    this.quad([x1, y0, z0], [x0, y0, z0], [x0, y1, z0], [x1, y1, z0], col, c, w, h, kind);
    this.quad([x1, y0, z1], [x1, y0, z0], [x1, y1, z0], [x1, y1, z1], col, c, d, h, kind);
    this.quad([x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0], col, c, d, h, kind);
    this.quad([x0, y1, z1], [x1, y1, z1], [x1, y1, z0], [x0, y1, z0], top || col, c, w, d, topKind || 0);
  };
  // A gabled roof over x0..x1, z0..z1 at y, ridge along the longer side.
  Mesh.prototype.gable = function (x0, z0, x1, z1, y, rise, col, wall) {
    var c = [(x0 + x1) / 2, y + rise / 3, (z0 + z1) / 2], o = 0.6;
    if (x1 - x0 >= z1 - z0) {
      var zm = (z0 + z1) / 2;
      this.quad([x0 - o, y, z1 + o], [x1 + o, y, z1 + o], [x1 + o, y + rise, zm], [x0 - o, y + rise, zm], col, c, 1, 1, 0);
      this.quad([x1 + o, y, z0 - o], [x0 - o, y, z0 - o], [x0 - o, y + rise, zm], [x1 + o, y + rise, zm], col, c, 1, 1, 0);
      this.tri([x1, y, z1], [x1, y, z0], [x1, y + rise, zm], wall, c, [0, 0], [1, 0], [0.5, 1], 0);
      this.tri([x0, y, z0], [x0, y, z1], [x0, y + rise, zm], wall, c, [0, 0], [1, 0], [0.5, 1], 0);
    } else {
      var xm = (x0 + x1) / 2;
      this.quad([x1 + o, y, z1 + o], [x1 + o, y, z0 - o], [xm, y + rise, z0 - o], [xm, y + rise, z1 + o], col, c, 1, 1, 0);
      this.quad([x0 - o, y, z0 - o], [x0 - o, y, z1 + o], [xm, y + rise, z1 + o], [xm, y + rise, z0 - o], col, c, 1, 1, 0);
      this.tri([x0, y, z1], [x1, y, z1], [xm, y + rise, z1], wall, c, [0, 0], [1, 0], [0.5, 1], 0);
      this.tri([x1, y, z0], [x0, y, z0], [xm, y + rise, z0], wall, c, [0, 0], [1, 0], [0.5, 1], 0);
    }
  };
  // A lathed shape: profile [[radius, y], ...] bottom to top round (cx, cz).
  Mesh.prototype.lathe = function (cx, by, cz, prof, sides, col, kind) {
    var centre = [cx, by + prof[prof.length - 1][1] / 2, cz];
    for (var i = 0; i < prof.length - 1; i++) {
      var r0 = prof[i][0], y0 = by + prof[i][1], r1 = prof[i + 1][0], y1 = by + prof[i + 1][1];
      var colr = typeof col === "function" ? col(i) : col;
      var kd = typeof kind === "function" ? kind(i) : kind;
      // Smooth normals across the sides: slope of this ring.
      var dr = r1 - r0, dy = y1 - y0, nl = Math.hypot(dr, dy) || 1;
      var ny = -dr / nl, nr = dy / nl;
      for (var s = 0; s < sides; s++) {
        var a0 = (s / sides) * Math.PI * 2, a1 = ((s + 1) / sides) * Math.PI * 2;
        var c0 = Math.cos(a0), s0 = Math.sin(a0), c1 = Math.cos(a1), s1 = Math.sin(a1);
        var p00 = [cx + c0 * r0, y0, cz + s0 * r0], p10 = [cx + c1 * r0, y0, cz + s1 * r0];
        var p01 = [cx + c0 * r1, y1, cz + s0 * r1], p11 = [cx + c1 * r1, y1, cz + s1 * r1];
        var n0 = [c0 * nr, ny, s0 * nr], n1 = [c1 * nr, ny, s1 * nr];
        var o = function (p) { var d = [p[0] - centre[0], p[1] - centre[1], p[2] - centre[2]]; var l = Math.hypot(d[0], d[1], d[2]) || 1; return [d[0] / l, d[1] / l, d[2] / l]; };
        var u0 = (s / sides) * Math.PI * 2 * Math.max(r0, 1), u1 = ((s + 1) / sides) * Math.PI * 2 * Math.max(r0, 1);
        this.vert(p00, n0, o(p00), colr, u0, y0 - by, kd);
        this.vert(p11, n1, o(p11), colr, u1, y1 - by, kd);
        this.vert(p10, n1, o(p10), colr, u1, y0 - by, kd);
        this.vert(p00, n0, o(p00), colr, u0, y0 - by, kd);
        this.vert(p01, n0, o(p01), colr, u0, y1 - by, kd);
        this.vert(p11, n1, o(p11), colr, u1, y1 - by, kd);
      }
    }
  };
  function ellipse(rx, ry, n) {
    var p = [];
    for (var i = 0; i <= n; i++) { var a = -Math.PI / 2 + (i / n) * Math.PI; p.push([Math.cos(a) * rx, ry + Math.sin(a) * ry]); }
    p[0][0] = 0.001; p[n][0] = 0.001;
    return p;
  }

  // ------------------------------------------------------------- palette
  var C = {
    cream: hex("#f3dfb5"), ochre: hex("#e3b86a"), brick: hex("#b85a3c"), plaster: hex("#e9d2c0"),
    roofRed: hex("#c4402f"), roofSlate: hex("#4b5a7a"), roofRust: hex("#a8542e"),
    teal: hex("#4fb3a9"), salmon: hex("#ef8f74"), mint: hex("#a6dcc0"), sky: hex("#8fc6e8"), white: hex("#f6f1e6"),
    chrome: hex("#d8dde6"), gold: hex("#ffc640"), red: hex("#e23430"), navy: hex("#161a40"),
    aether: hex("#78ecff"), villain: hex("#4b4560"), villainDark: hex("#2f2b3e"), steel: hex("#8a8fa0"),
    green: hex("#6fae58"), concrete: hex("#cfc8b8")
  };
  var CAR_COLOURS = ["#e23430", "#4fb3a9", "#ffc640", "#ef8f74", "#8fc6e8", "#f6f1e6", "#a6dcc0", "#d06aa0"].map(hex);

  // -------------------------------------------------------------- the town
  var PITCH = 44, ROAD = 9, REACH = 9;
  var QUARTER = { a0: 3.4, a1: 4.35, r0: 150, r1: 380 }; // the villain quarter's sector
  function inQuarter(x, z) {
    var a = Math.atan2(z, x); if (a < 0) a += Math.PI * 2;
    var r = Math.hypot(x, z);
    return a > QUARTER.a0 && a < QUARTER.a1 && r > QUARTER.r0 && r < QUARTER.r1;
  }
  var TESLA = [Math.cos(3.85) * 265, Math.sin(3.85) * 265];
  var ROCKET = [Math.cos(0.75) * 470, Math.sin(0.75) * 470];
  var ATOM = [Math.cos(5.3) * 500, Math.sin(5.3) * 500];
  var MAST = [Math.cos(2.2) * 480, Math.sin(2.2) * 480];

  function buildTown() {
    var m = new Mesh(), R = rng(1950);
    var reach = REACH * PITCH;
    // Ground: one big square, the shader draws streets, lawns and fields.
    var G = 2600;
    m.quad([-G, 0, G], [G, 0, G], [G, 0, -G], [-G, 0, -G], C.green, [0, -10, 0], 1, 1, 4);
    for (var i = -REACH; i <= REACH; i++) for (var j = -REACH; j <= REACH; j++) {
      var bx = i * PITCH, bz = j * PITCH, r = Math.hypot(bx, bz);
      if (r > reach) continue;
      if (r < 30) continue; // the spire's plaza
      var half = (PITCH - ROAD) / 2 - 1;
      // Parks: the shader paints them; here a few trees.
      if (blockHash(i, j) < 0.08) { trees(m, bx, bz, half, R); continue; }
      var villain = inQuarter(bx, bz);
      if (Math.hypot(bx - TESLA[0], bz - TESLA[1]) < 40) continue;
      var old = r < 215 && !villain;
      var n = old ? 2 + Math.floor(R() * 2) : 1 + Math.floor(R() * 2);
      var lot = (half * 2) / n;
      for (var a = 0; a < n; a++) for (var b = 0; b < n; b++) {
        var x0 = bx - half + a * lot + 0.8, z0 = bz - half + b * lot + 0.8;
        var x1 = x0 + lot - 1.6, z1 = z0 + lot - 1.6;
        if (villain) villainBlock(m, x0, z0, x1, z1, R);
        else if (old) oldHouse(m, x0, z0, x1, z1, R);
        else modern(m, x0, z0, x1, z1, R, r);
      }
    }
    spire(m);
    tesla(m);
    rocketPad(m);
    atomic(m);
    mast(m);
    return new Float32Array(m.v);
  }
  function blockHash(i, j) {
    var h = Math.sin(i * 127.1 + j * 311.7) * 43758.5453;
    return h - Math.floor(h);
  }
  function trees(m, bx, bz, half, R) {
    for (var k = 0; k < 7; k++) {
      var x = bx + (R() * 2 - 1) * (half - 4), z = bz + (R() * 2 - 1) * (half - 4), s = 3 + R() * 2.5;
      m.lathe(x, 0, z, [[0.5, 0], [0.4, s * 0.8]], 6, hex("#7a5236"), 0);
      m.lathe(x, s * 0.7, z, ellipse(s * 0.75, s * 0.8, 6), 9, hex(R() < 0.5 ? "#4f9a48" : "#5fae4e"), 0);
    }
  }
  function oldHouse(m, x0, z0, x1, z1, R) {
    var walls = [C.cream, C.ochre, C.plaster, C.brick, hex("#d9c39a")];
    var wall = walls[Math.floor(R() * walls.length)];
    var floors = 3 + Math.floor(R() * 3), h = floors * 3.3;
    m.box(x0, 0, z0, x1, h, z1, wall, 1);
    var roofs = [C.roofRed, C.roofRed, C.roofRust, C.roofSlate];
    m.gable(x0, z0, x1, z1, h, 4 + R() * 3, roofs[Math.floor(R() * roofs.length)], wall);
  }
  function modern(m, x0, z0, x1, z1, R, r) {
    var cols = [C.white, C.teal, C.salmon, C.mint, C.sky, C.cream];
    var col = cols[Math.floor(R() * cols.length)];
    var tall = Math.max(0, 1 - (r - 200) / 200);
    var floors = 3 + Math.floor(R() * (3 + tall * 9)), h = floors * 3.4;
    var inset = R() < 0.4 ? 3 : 0;
    m.box(x0 + inset, 0, z0 + inset, x1 - inset, h, z1 - inset, col, 1, C.concrete);
    // A streamline band and, now and then, a Googie fin or a setback top.
    m.box(x0 + inset - 0.4, h * 0.33, z0 + inset - 0.4, x1 - inset + 0.4, h * 0.33 + 1.1, z1 - inset + 0.4, C.white, 0);
    if (floors > 10 && R() < 0.3) {
      m.box(x0 + inset + 4, h, z0 + inset + 4, x1 - inset - 4, h + 9, z1 - inset - 4, col, 1, C.concrete);
      m.lathe((x0 + x1) / 2, h + 9, (z0 + z1) / 2, [[0.4, 0], [0.15, 10], [0.01, 10.2]], 6, C.chrome, 0);
      m.lathe((x0 + x1) / 2, h + 19, (z0 + z1) / 2, ellipse(1.2, 1.2, 6), 8, C.red, 2);
    } else if (R() < 0.12) {
      var fx = (x0 + x1) / 2;
      m.box(fx - 0.5, h, z0 + inset, fx + 0.5, h + 7, z1 - inset, C.red, 0);
    }
  }
  function villainBlock(m, x0, z0, x1, z1, R) {
    var floors = 4 + Math.floor(R() * 6), h = floors * 3.4;
    m.box(x0, 0, z0, x1, h, z1, R() < 0.5 ? C.villain : C.villainDark, 3, hex("#3a3548"));
    if (R() < 0.5) {
      var cx = x0 + (x1 - x0) * 0.3, cz = z0 + (z1 - z0) * 0.7;
      m.lathe(cx, h, cz, [[1.6, 0], [1.2, 14], [1.4, 15]], 10, hex("#5d5468"), 0);
    }
  }
  function spire(m) {
    // The Aether Spire: a tapering chrome tower with glowing rings.
    m.lathe(0, 0, 0, [[14, 0], [12, 3], [9, 4], [8, 30], [6, 34], [5, 80], [3.5, 84], [2.6, 120], [0.4, 150]], 24,
      function (i) { return i % 2 ? C.white : C.chrome; }, 0);
    for (var k = 0; k < 6; k++) {
      var y = 36 + k * 16, r = 7.5 - k * 0.9;
      m.lathe(0, y, 0, [[r, 0], [r + 1.4, 0.8], [r, 1.6]], 24, C.aether, 2);
    }
    m.lathe(0, 148, 0, ellipse(3, 3, 8), 12, C.aether, 2);
  }
  function tesla(m) {
    var x = TESLA[0], z = TESLA[1];
    m.box(x - 18, 0, z - 14, x + 18, 12, z + 14, C.villainDark, 3, hex("#3a3548"));
    m.lathe(x, 12, z, [[7, 0], [6, 4], [3, 10], [2.2, 50], [1.4, 56]], 16, function (i) { return i % 2 ? C.steel : hex("#6b6478"); }, 0);
    for (var k = 0; k < 7; k++) m.lathe(x, 24 + k * 4.4, z, [[3, 0], [3.9, 0.9], [3, 1.8]], 16, hex("#b07a40"), 0);
    m.lathe(x, 64, z, ellipse(6, 6, 10), 18, C.chrome, 0);
  }
  function rocketPad(m) {
    var x = ROCKET[0], z = ROCKET[1];
    m.lathe(x, 0, z, [[22, 0], [22, 2], [0.1, 2]], 24, C.concrete, 0);
    // The gantry: a red lattice tower (boxes).
    var gx = x + 11;
    for (var k = 0; k < 9; k++) m.box(gx - 3, 2 + k * 6, z - 3, gx + 3, 2.6 + k * 6, z + 3, C.red, 0);
    [[-3, -3], [3, -3], [-3, 3], [3, 3]].forEach(function (o) { m.box(gx + o[0] - 0.4, 2, z + o[1] - 0.4, gx + o[0] + 0.4, 56, z + o[1] + 0.4, C.red, 0); });
    m.box(gx - 9, 40, z - 0.6, gx - 3, 41, z + 0.6, C.red, 0);
  }
  function atomic(m) {
    var x = ATOM[0], z = ATOM[1];
    var cool = [[18, 0], [15, 12], [11.5, 30], [12.5, 40], [13.5, 46]];
    m.lathe(x - 26, 0, z, cool, 24, C.concrete, 0);
    m.lathe(x + 12, 0, z - 22, cool, 24, C.concrete, 0);
    m.lathe(x + 10, 0, z + 22, [[0.1, 0]].concat(ellipse(14, 12, 8).slice(4)), 20, C.white, 0);
    m.box(x - 6, 0, z + 34, x + 30, 8, z + 46, C.mint, 1, C.concrete);
    m.lathe(x + 34, 0, z + 6, [[2.4, 0], [1.6, 60]], 10, function (i) { return C.red; }, 0);
    m.lathe(x + 34, 40, z + 6, [[2, 0], [2, 6]], 10, C.white, 0);
  }
  function mast(m) {
    var x = MAST[0], z = MAST[1];
    m.lathe(x, 0, z, [[6, 0], [3, 8], [1.6, 60], [2.8, 62], [2.8, 66], [0.6, 70]], 12, C.chrome, 0);
    m.box(x - 30, 0, z - 10, x - 10, 7, z + 10, C.cream, 1, C.roofRed);
  }

  // ------------------------------------------------------ moving things
  function carMesh() {
    var m = new Mesh();
    m.box(-1.0, 0.35, -2.4, 1.0, 1.05, 2.4, [1, 1, 1], 0, null, 0);
    m.lathe(0, 1.0, -0.2, ellipse(0.85, 0.55, 5).slice(2), 10, hex("#bfe8f4"), 5);
    m.box(-1.08, 0.3, 2.35, 1.08, 0.55, 2.6, C.chrome, 0);
    m.box(-1.08, 0.3, -2.6, 1.08, 0.55, -2.35, C.chrome, 0);
    return new Float32Array(m.v);
  }
  function taxiMesh() {
    var m = new Mesh();
    m.lathe(0, 0, 0, [[0.1, 0], [2.2, 0.5], [2.6, 0.9], [2.2, 1.3], [0.1, 1.4]], 18, [1, 1, 1], 0);
    m.lathe(0, 1.2, 0, ellipse(1.2, 0.9, 6).slice(3), 14, hex("#bfe8f4"), 5);
    m.lathe(0, -0.3, 0, ellipse(0.7, 0.3, 4), 10, C.aether, 2);
    return new Float32Array(m.v);
  }
  function airshipMesh() {
    var m = new Mesh();
    // Built along +Y, then turned to lie along +Z by the shader's pitch.
    var hull = [];
    for (var i = 0; i <= 16; i++) { var t = i / 16; hull.push([Math.sin(t * Math.PI) * 9 * (1 - 0.25 * t) + 0.01, t * 60]); }
    m.lathe(0, -30, 0, hull, 20, function (i) { return i % 4 < 2 ? hex("#efe2c4") : hex("#e6d4b0"); }, 0);
    // Fins at the tail (low y), gondola below the middle.
    [[1, 0], [-1, 0], [0, 1], [0, -1]].forEach(function (d) {
      var a = [d[0] * 4, -26, d[1] * 4], b = [d[0] * 13, -31, d[1] * 13], c = [d[0] * 4, -16, d[1] * 4];
      m.tri(a, b, c, C.red, [0, -24, 0], [0, 0], [1, 0], [0, 1], 0);
      m.tri(a, c, b, C.red, [0, -24, 0], [0, 0], [1, 0], [0, 1], 0);
    });
    m.box(-2.5, -6, 8.5, 2.5, 6, 11, hex("#7a4a2a"), 1);
    return new Float32Array(m.v);
  }
  function rocketMesh() {
    var m = new Mesh();
    var prof = [[2.6, 0], [3.2, 4], [3.4, 14], [3.2, 26], [2.4, 34], [1.2, 40], [0.05, 44]];
    m.lathe(0, 0, 0, prof, 20, function (i) { return i === 4 || i === 5 ? C.red : C.white; }, 0);
    [0, 1, 2].forEach(function (k) {
      var a = (k / 3) * Math.PI * 2, c = Math.cos(a), s = Math.sin(a);
      var p = [c * 3, 2, s * 3], q = [c * 7, -1, s * 7], r = [c * 3, 12, s * 3];
      m.tri(p, q, r, C.red, [0, 6, 0], [0, 0], [1, 0], [0, 1], 0);
      m.tri(p, r, q, C.red, [0, 6, 0], [0, 0], [1, 0], [0, 1], 0);
    });
    m.lathe(0, 30, 0, [[2.0, 0], [2.0, 2]], 12, C.aether, 5);
    return new Float32Array(m.v);
  }
  function heroMesh() {
    var m = new Mesh();
    // Lying along +Y (head first): body, head, and a red cape behind.
    m.lathe(0, -1.0, 0, ellipse(0.42, 1.0, 6), 8, hex("#2f4fb0"), 0);
    m.lathe(0, 1.05, 0, ellipse(0.3, 0.3, 5), 8, hex("#f0c09a"), 0);
    m.tri([-0.5, 0.8, -0.3], [0.5, 0.8, -0.3], [0, -2.6, -0.9], C.red, [0, 0, 0], [0, 0], [1, 0], [0, 1], 0);
    m.tri([0.5, 0.8, -0.3], [-0.5, 0.8, -0.3], [0, -2.6, -0.9], C.red, [0, 0, 0], [0, 0], [1, 0], [0, 1], 0);
    return new Float32Array(m.v);
  }

  // ------------------------------------------------------------- shaders
  var VS = "#version 300 es\n" +
    "precision highp float;\n" +
    "layout(location=0) in vec3 a_pos; layout(location=1) in vec3 a_nrm; layout(location=2) in vec3 a_out;\n" +
    "layout(location=3) in vec3 a_col; layout(location=4) in vec3 a_uvk;\n" +
    "layout(location=5) in vec4 i_place; layout(location=6) in vec4 i_look;\n" +
    "uniform mat4 u_vp; uniform vec3 u_cam; uniform float u_outline;\n" +
    "out vec3 v_pos; out vec3 v_nrm; out vec3 v_col; out vec3 v_uvk;\n" +
    // i_place: x y z yaw; i_look: scale, pitch, tint index (as colour packed), unused
    "mat3 rot(float yaw, float pitch){ float cy=cos(yaw), sy=sin(yaw), cp=cos(pitch), sp=sin(pitch);\n" +
    "  mat3 p = mat3(1.,0.,0., 0.,cp,sp, 0.,-sp,cp); mat3 y = mat3(cy,0.,-sy, 0.,1.,0., sy,0.,cy); return y*p; }\n" +
    "vec3 tint(float k){ int i=int(k); \n" +
    "  if(i==0) return vec3(1.); if(i==1) return vec3(0.886,0.204,0.188); if(i==2) return vec3(0.31,0.70,0.66);\n" +
    "  if(i==3) return vec3(1.0,0.776,0.25); if(i==4) return vec3(0.94,0.56,0.45); if(i==5) return vec3(0.56,0.78,0.91);\n" +
    "  if(i==6) return vec3(0.96,0.94,0.90); if(i==7) return vec3(0.65,0.86,0.75); return vec3(0.82,0.42,0.63); }\n" +
    "void main(){\n" +
    "  mat3 r = rot(i_place.w, i_look.y);\n" +
    "  vec3 p = r * (a_pos * i_look.x) + i_place.xyz;\n" +
    "  vec3 n = normalize(r * a_nrm);\n" +
    "  if (u_outline > 0.0) { float d = length(p - u_cam); p += normalize(r * a_out) * u_outline * d; }\n" +
    "  v_pos = p; v_nrm = n; v_uvk = a_uvk;\n" +
    "  v_col = (a_col.r > 0.99 && a_col.g > 0.99 && a_col.b > 0.99) ? tint(i_look.z) : a_col;\n" +
    "  gl_Position = u_vp * vec4(p, 1.0);\n" +
    "}\n";

  var COMMON = "#version 300 es\nprecision highp float;\n" +
    "uniform vec3 u_cam; uniform vec3 u_sun; uniform float u_time; uniform vec2 u_px;\n" +
    "float h21(vec2 p){ vec3 q=fract(vec3(p.xyx)*0.1031); q+=dot(q,q.yzx+33.33); return fract((q.x+q.y)*q.z); }\n" +
    "vec3 skyCol(vec3 d){ float h=max(d.y,0.0);\n" +
    "  vec2 dh=normalize(d.xz+1e-5); float ts=dot(dh, normalize(u_sun.xz))*0.5+0.5;\n" +
    "  vec3 hor=mix(vec3(1.0,0.86,0.66), vec3(1.0,0.70,0.46), ts*ts);\n" +
    "  vec3 c=mix(hor, vec3(0.50,0.72,0.88), smoothstep(0.0,0.28,h)); c=mix(c, vec3(0.26,0.48,0.78), smoothstep(0.25,0.9,h));\n" +
    "  return c; }\n" +
    "vec3 fogged(vec3 c, vec3 p){ float d=length(p-u_cam); float f=1.0-exp(-pow(d/1500.0,1.7)); vec3 dir=normalize(p-u_cam); return mix(c, skyCol(vec3(dir.x,0.02,dir.z)), f*0.85); }\n" +
    // halftone: dots on a 45-degree screen grid, radius grows with `dark`
    "float halftone(float dark){ vec2 g=gl_FragCoord.xy*u_px.y; vec2 r=mat2(0.707,-0.707,0.707,0.707)*g/7.0; vec2 f=fract(r)-0.5; return 1.0-smoothstep(dark*0.62-0.06, dark*0.62+0.06, length(f)); }\n";

  var FS = COMMON +
    "in vec3 v_pos; in vec3 v_nrm; in vec3 v_col; in vec3 v_uvk;\n" +
    "uniform float u_ink;\n" +
    "out vec4 o;\n" +
    "vec3 ground(vec2 p){\n" +
    "  float r=length(p); vec2 m=mod(p+" + (PITCH / 2).toFixed(1) + "," + PITCH.toFixed(1) + "); vec2 id=floor((p+" + (PITCH / 2).toFixed(1) + ")/" + PITCH.toFixed(1) + ");\n" +
    "  float road=" + (ROAD / 2).toFixed(1) + ";\n" +
    "  bool street = (m.x<road || m.x>" + PITCH.toFixed(1) + "-road || m.y<road || m.y>" + PITCH.toFixed(1) + "-road);\n" +
    "  if (r > " + (REACH * PITCH + 22).toFixed(1) + ") { float f=h21(floor(p/90.0)); return mix(vec3(0.52,0.68,0.38), vec3(0.74,0.72,0.42), step(0.65,f)); }\n" +
    "  if (r < 30.0) { float ring=step(0.5,fract(r/6.0)); return mix(vec3(0.93,0.86,0.72), vec3(0.85,0.36,0.30), ring*step(r,24.0)*0.35); }\n" +
    "  if (street) { vec2 c=abs(m-" + (PITCH / 2).toFixed(1) + "); float dash = step(" + (PITCH / 2 - 0.25).toFixed(2) + ", max(c.x,c.y)) * step(0.5, fract((m.x+m.y)/6.0));\n" +
    "    return mix(vec3(0.36,0.38,0.46), vec3(1.0,0.86,0.40), dash*0.0); }\n" +
    "  float park = step(h21(id*1.0+vec2(0.0))*0.0 + fract(sin(id.x*127.1+id.y*311.7)*43758.5453), 0.08);\n" +
    "  vec2 e=min(m, " + PITCH.toFixed(1) + "-m); float kerb=step(min(e.x,e.y), road+2.0);\n" +
    "  vec3 pave=vec3(0.82,0.78,0.70); vec3 lawn=vec3(0.45,0.70,0.36);\n" +
    "  return park>0.5 ? mix(lawn, pave, kerb) : pave; }\n" +
    "void main(){\n" +
    "  if (u_ink > 0.0) { o = vec4(0.086,0.102,0.25,1.0); return; }\n" +
    "  int k=int(v_uvk.z+0.5); vec3 n=normalize(v_nrm); vec3 col=v_col;\n" +
    "  float glow=0.0;\n" +
    "  if (k==4) { col = ground(v_pos.xz); }\n" +
    "  if (k==1 || k==3) {\n" +
    "    vec2 cell=vec2(v_uvk.x/3.2, v_uvk.y/3.3); vec2 f=fract(cell); vec2 id=floor(cell);\n" +
    "    float win = step(0.24,f.x)*step(f.x,0.76)*step(0.28,f.y)*step(f.y,0.80)*step(0.9,v_uvk.y);\n    float frame = win - step(0.30,f.x)*step(f.x,0.70)*step(0.34,f.y)*step(f.y,0.74)*step(0.9,v_uvk.y);\n" +
    "    float lit = h21(id + floor(v_pos.xz*0.05));\n    float near = 1.0 - smoothstep(260.0, 520.0, length(v_pos-u_cam)); win *= near; frame *= near;\n" +
    "    if (k==1) { vec3 glass = mix(vec3(0.23,0.32,0.45), vec3(0.62,0.80,0.92), step(0.55,f.y));\n" +
    "      glass = mix(glass, vec3(1.0,0.82,0.45), step(0.86,lit)); col = mix(col, glass, win); col = mix(col, col*0.55+vec3(0.05,0.05,0.12), frame); }\n" +
    "    else { vec3 gl3 = mix(vec3(0.12,0.12,0.18), vec3(0.45,1.0,0.55), step(0.7,lit)*(0.75+0.25*sin(u_time*3.0+lit*40.0)));\n" +
    "      col = mix(col, gl3, win); glow = win*step(0.7,lit); }\n" +
    "  }\n" +
    "  if (k==2) { float pulse = 0.75+0.25*sin(u_time*2.5 - v_pos.y*0.15); o = vec4(fogged(col*(1.1+0.6*pulse), v_pos),1.0); return; }\n" +
    "  float l = dot(n, u_sun);\n" +
    "  float band = l > 0.3 ? 1.0 : (l > -0.1 ? 0.84 : 0.64);\n" +
    "  if (k==4) band = 1.0;\n" +
    "  vec3 lit3 = col * band * vec3(1.06,0.98,0.88) + col*vec3(0.06,0.08,0.14);\n" +
    "  if (k==5) { vec3 v=normalize(u_cam-v_pos); float fr=pow(1.0-max(dot(n,v),0.0),3.0); lit3 = mix(lit3, vec3(1.0), fr*0.6+step(0.92, dot(reflect(-v,n),u_sun))*0.8); }\n" +
    "  if (band < 0.7) lit3 = mix(lit3, lit3*0.6, halftone(0.55));\n" +
    "  lit3 = mix(lit3, col*1.3, glow);\n" +
    "  o = vec4(fogged(lit3, v_pos), 1.0);\n" +
    "}\n";

  var SKY_VS = "#version 300 es\nout vec2 v_ndc; void main(){ vec2 p=vec2((gl_VertexID<<1)&2, gl_VertexID&2); v_ndc=p*2.0-1.0; gl_Position=vec4(v_ndc,0.9999,1.0);}\n";
  var SKY_FS = COMMON +
    "in vec2 v_ndc; uniform mat4 u_inv; out vec4 o;\n" +
    "void main(){ vec4 w=u_inv*vec4(v_ndc,1.0,1.0); vec3 d=normalize(w.xyz/w.w-u_cam);\n" +
    "  vec3 c=skyCol(d);\n" +
    // comic sunburst round the sun, faint
    "  vec3 s=normalize(u_sun); float g=max(dot(d,s),0.0);\n" +
    "  vec3 side=normalize(cross(s,vec3(0.0,1.0,0.0))); vec3 up2=cross(side,s); float ang=atan(dot(d,up2),dot(d,side));\n" +
    "  float rays=step(0.5,fract(ang*3.0+u_time*0.01)); c=mix(c, c*vec3(1.08,1.03,0.95), rays*smoothstep(0.55,0.95,g)*0.6);\n" +
    "  c=mix(c, vec3(1.0,0.95,0.78), smoothstep(0.9965,0.9975,g));\n" +
    "  c+=vec3(1.0,0.6,0.3)*pow(g,30.0)*0.25;\n" +
    // two moons, pale, with an ink rim
    "  vec3 m1=normalize(vec3(0.55,0.42,0.72)); vec3 m2=normalize(vec3(0.70,0.30,0.64));\n" +
    "  float a1=dot(d,m1), a2=dot(d,m2);\n" +
    "  c=mix(c, vec3(0.086,0.10,0.25), smoothstep(0.99935,0.9995,a1)); c=mix(c, vec3(0.97,0.95,0.90), smoothstep(0.99950,0.9996,a1));\n" +
    "  c=mix(c, vec3(0.086,0.10,0.25), smoothstep(0.99975,0.99980,a2)); c=mix(c, vec3(0.99,0.88,0.80), smoothstep(0.99982,0.99986,a2));\n" +
    // halftone dots high in the sky
    "  c=mix(c, c*0.93, halftone(smoothstep(0.2,0.9,d.y)*0.35));\n" +
    "  o=vec4(c,1.0);}\n";

  var LINE_VS = "#version 300 es\nlayout(location=0) in vec4 a_p; uniform mat4 u_vp; uniform float u_size; out float v_a;\n" +
    "void main(){ gl_Position=u_vp*vec4(a_p.xyz,1.0); v_a=a_p.w; gl_PointSize=u_size*a_p.w/gl_Position.w; }\n";
  var LINE_FS = "#version 300 es\nprecision highp float; in float v_a; uniform vec4 u_col; uniform float u_points; out vec4 o;\n" +
    "void main(){ float a=1.0; if(u_points>0.5){ vec2 q=gl_PointCoord-0.5; float r=length(q); if(r>0.5) discard; a = r>0.44 ? 1.0 : 0.0;\n" +
    "  o = r>0.42 ? vec4(0.086,0.10,0.25,min(1.0,v_a*1.5)) : vec4(u_col.rgb, u_col.a); return; }\n" +
    "  o=vec4(u_col.rgb, u_col.a*clamp(v_a,0.0,1.0)); }\n";

  // ----------------------------------------------------------------- GL
  function compile(gl, vs, fs) {
    var p = gl.createProgram();
    [[gl.VERTEX_SHADER, vs], [gl.FRAGMENT_SHADER, fs]].forEach(function (s) {
      var sh = gl.createShader(s[0]); gl.shaderSource(sh, s[1]); gl.compileShader(sh);
      if (!gl.getShaderParameter(sh, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(sh));
      gl.attachShader(p, sh);
    });
    gl.linkProgram(p);
    if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(p));
    var u = {}, n = gl.getProgramParameter(p, gl.ACTIVE_UNIFORMS);
    for (var i = 0; i < n; i++) { var info = gl.getActiveUniform(p, i); u[info.name] = gl.getUniformLocation(p, info.name); }
    return { p: p, u: u };
  }

  function start(canvas, opts) {
    opts = opts || {};
    var gl = canvas.getContext("webgl2", { antialias: true, alpha: false });
    if (!gl) return false;
    var prog, sky, lines;
    try {
      prog = compile(gl, VS, FS);
      sky = compile(gl, SKY_VS, SKY_FS);
      lines = compile(gl, LINE_VS, LINE_FS);
    } catch (e) {
      if (window.console) console.warn("scene:", e.message);
      return false;
    }
    var still = !!opts.still;

    // One VAO per mesh; instances in a dynamic buffer per mesh.
    function meshVao(data, maxInst) {
      var vao = gl.createVertexArray(); gl.bindVertexArray(vao);
      var vb = gl.createBuffer(); gl.bindBuffer(gl.ARRAY_BUFFER, vb); gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW);
      var offs = [0, 3, 6, 9, 12];
      for (var a = 0; a < 5; a++) { gl.enableVertexAttribArray(a); gl.vertexAttribPointer(a, 3, gl.FLOAT, false, STRIDE * 4, offs[a] * 4); }
      var ib = gl.createBuffer(); gl.bindBuffer(gl.ARRAY_BUFFER, ib); gl.bufferData(gl.ARRAY_BUFFER, maxInst * 32, gl.DYNAMIC_DRAW);
      gl.enableVertexAttribArray(5); gl.vertexAttribPointer(5, 4, gl.FLOAT, false, 32, 0); gl.vertexAttribDivisor(5, 1);
      gl.enableVertexAttribArray(6); gl.vertexAttribPointer(6, 4, gl.FLOAT, false, 32, 16); gl.vertexAttribDivisor(6, 1);
      gl.bindVertexArray(null);
      return { vao: vao, ib: ib, count: data.length / STRIDE, inst: new Float32Array(maxInst * 8), n: 0 };
    }
    var town = meshVao(buildTown(), 1);
    town.inst.set([0, 0, 0, 0, 1, 0, 0, 0]); town.n = 1;
    var cars = meshVao(carMesh(), 200), taxis = meshVao(taxiMesh(), 12);
    var ship = meshVao(airshipMesh(), 1), rocket = meshVao(rocketMesh(), 1), hero = meshVao(heroMesh(), 1);

    // Cars: each on one street line, going one way along it.
    var R = rng(77), carList = [];
    for (var i = 0; i < 170; i++) {
      var along = R() < 0.5, line = Math.round((R() * 2 - 1) * (REACH - 1)) * PITCH + PITCH / 2;
      var dir = R() < 0.5 ? 1 : -1;
      carList.push({ along: along, line: line + dir * 2.2, dir: dir, s: (R() * 2 - 1) * REACH * PITCH, v: 9 + R() * 6, tint: 1 + Math.floor(R() * 8) });
    }
    var taxiList = [];
    for (var t = 0; t < 9; t++) taxiList.push({ r: 120 + R() * 300, h: 55 + R() * 45, w: (R() < 0.5 ? 1 : -1) * (0.05 + R() * 0.05), p: R() * 7, k: 2 + R() * 2, tint: t % 3 === 0 ? 3 : (t % 3 === 1 ? 6 : 1) });

    // Arcs, smoke and the hero's trail: points and lines, rebuilt each frame.
    var fx = gl.createBuffer(), fxVao = gl.createVertexArray();
    gl.bindVertexArray(fxVao); gl.bindBuffer(gl.ARRAY_BUFFER, fx); gl.bufferData(gl.ARRAY_BUFFER, 4 * 4 * 4000, gl.DYNAMIC_DRAW);
    gl.enableVertexAttribArray(0); gl.vertexAttribPointer(0, 4, gl.FLOAT, false, 16, 0); gl.bindVertexArray(null);
    var skyVao = gl.createVertexArray();

    var sun = (function () { var s = [-0.55, 0.52, -0.66], l = Math.hypot(s[0], s[1], s[2]); return [s[0] / l, s[1] / l, s[2] / l]; })();
    var qt = /[?&]t=(\d+(?:\.\d+)?)/.exec(location.search);
    var mouse = [0, 0], lean = [0, 0], time = qt ? Number(qt[1]) : (opts.time || 30), last = 0, raf = 0, visible = true, scale = 1, slow = 0, frames = 0;
    var heroTrail = [];
    window.addEventListener("pointermove", function (e) { mouse = [e.clientX / innerWidth * 2 - 1, e.clientY / innerHeight * 2 - 1]; });
    if ("IntersectionObserver" in window) {
      new IntersectionObserver(function (es) { visible = es[0].isIntersecting; if (visible && !raf && !still) raf = requestAnimationFrame(frame); }).observe(canvas);
    }

    function heroAt(t) {
      // A looping path over the roofs: a lazy figure eight round the spire.
      var a = t * 0.22;
      return [Math.sin(a) * 210, 70 + Math.sin(a * 2.0) * 18, Math.sin(a * 2.0) * 120];
    }

    function draw() {
      var dpr = Math.min(window.devicePixelRatio || 1, 2) * scale;
      var w = Math.max(1, Math.round(canvas.clientWidth * dpr)), h = Math.max(1, Math.round(canvas.clientHeight * dpr));
      if (canvas.width !== w || canvas.height !== h) { canvas.width = w; canvas.height = h; }
      gl.viewport(0, 0, w, h);
      for (var q = 0; q < 2; q++) lean[q] += (mouse[q] - lean[q]) * 0.03;
      var a = time * 0.035 + lean[0] * 0.08;
      var rad = 330 + Math.sin(time * 0.05) * 40;
      var eye = [Math.cos(a) * rad, 105 + Math.sin(time * 0.07) * 20 - lean[1] * 12, Math.sin(a) * rad];
      var look = [Math.cos(a + 2.2) * 80, 48 - lean[1] * 20, Math.sin(a + 2.2) * 80];
      var aspect = w / h, fov = (aspect < 1 ? 70 : 50) * Math.PI / 180;
      var proj = perspective(fov, aspect, 1, 4000), view = lookAt(eye, look, [0, 1, 0]);
      var vp = mul(proj, view);

      // Sky: needs the inverse view-projection for ray directions.
      var inv = invert(vp);
      gl.disable(gl.DEPTH_TEST); gl.disable(gl.CULL_FACE);
      gl.useProgram(sky.p);
      gl.uniformMatrix4fv(sky.u.u_inv, false, inv); gl.uniform3fv(sky.u.u_cam, eye); gl.uniform3fv(sky.u.u_sun, sun);
      gl.uniform1f(sky.u.u_time, time); gl.uniform2f(sky.u.u_px, 1, 1 / dpr);
      gl.bindVertexArray(skyVao); gl.drawArrays(gl.TRIANGLES, 0, 3);

      gl.enable(gl.DEPTH_TEST); gl.depthFunc(gl.LEQUAL); gl.enable(gl.CULL_FACE);
      gl.clear(gl.DEPTH_BUFFER_BIT);
      gl.useProgram(prog.p);
      gl.uniformMatrix4fv(prog.u.u_vp, false, vp); gl.uniform3fv(prog.u.u_cam, eye); gl.uniform3fv(prog.u.u_sun, sun);
      gl.uniform1f(prog.u.u_time, time); gl.uniform2f(prog.u.u_px, 1, 1 / dpr);

      // Moving things this frame.
      var span = REACH * PITCH;
      cars.n = 0;
      carList.forEach(function (c) {
        var s = ((c.s + c.dir * c.v * time) % (2 * span) + 3 * span) % (2 * span) - span;
        var x = c.along ? s : c.line, z = c.along ? c.line : s;
        if (Math.hypot(x, z) > span - 10 || Math.hypot(x, z) < 34) return;
        var yaw = c.along ? (c.dir > 0 ? Math.PI / 2 : -Math.PI / 2) : (c.dir > 0 ? 0 : Math.PI);
        cars.inst.set([x, 0, z, yaw, 1, 0, c.tint, 0], cars.n * 8); cars.n++;
      });
      taxis.n = 0;
      taxiList.forEach(function (c) {
        var ang = c.p + time * c.w;
        var x = Math.cos(ang) * c.r, z = Math.sin(ang * c.k * 0.5) * c.r * 0.6;
        var dx = -Math.sin(ang) * c.r, dz = Math.cos(ang * c.k * 0.5) * c.r * 0.3 * c.k;
        taxis.inst.set([x, c.h + Math.sin(time + c.p) * 3, z, Math.atan2(dx * c.w, dz * c.w), 1.4, 0, c.tint, 0], taxis.n * 8); taxis.n++;
      });
      var sa = time * 0.025;
      ship.inst.set([Math.cos(sa) * 430, 120, Math.sin(sa) * 430, -sa, 1, Math.PI / 2, 0, 0]); ship.n = 1;
      // Rocket: waits 30 s on the pad, then 14 s of flight.
      var cyc = (time + 20) % 44, rise = cyc > 30 ? cyc - 30 : 0;
      var ry = 2 + rise * rise * 3.2;
      rocket.inst.set([ROCKET[0], ry, ROCKET[1], 0, 1, 0, 0, 0]); rocket.n = 1;
      var hp = heroAt(time), hq = heroAt(time + 0.05);
      var hd = [hq[0] - hp[0], hq[1] - hp[1], hq[2] - hp[2]];
      var hyaw = Math.atan2(hd[0], hd[2]), hpitch = Math.PI / 2 - Math.atan2(hd[1], Math.hypot(hd[0], hd[2])) * 0.5;
      hero.inst.set([hp[0], hp[1], hp[2], hyaw, 3.4, hpitch, 0, 0]); hero.n = 1;

      var meshes = [town, cars, taxis, ship, rocket, hero];
      meshes.forEach(function (m) { gl.bindBuffer(gl.ARRAY_BUFFER, m.ib); gl.bufferSubData(gl.ARRAY_BUFFER, 0, m.inst.subarray(0, m.n * 8)); });
      // Ink outlines first (back faces pushed out), then the colour pass.
      gl.cullFace(gl.FRONT); gl.uniform1f(prog.u.u_ink, 1); gl.uniform1f(prog.u.u_outline, 0.0026);
      meshes.forEach(function (m) { if (m.n) { gl.bindVertexArray(m.vao); gl.drawArraysInstanced(gl.TRIANGLES, 0, m.count, m.n); } });
      gl.cullFace(gl.BACK); gl.uniform1f(prog.u.u_ink, 0); gl.uniform1f(prog.u.u_outline, 0);
      meshes.forEach(function (m) { if (m.n) { gl.bindVertexArray(m.vao); gl.drawArraysInstanced(gl.TRIANGLES, 0, m.count, m.n); } });

      // Effects: Tesla arcs, the hero's streak, steam, rocket smoke.
      var arcs = [], R2 = rng(Math.floor(time * 12));
      for (var b = 0; b < 3; b++) {
        if (R2() < 0.35) continue;
        var p0 = [TESLA[0], 70, TESLA[1]], ang2 = R2() * Math.PI * 2, len = 25 + R2() * 35;
        var end = [TESLA[0] + Math.cos(ang2) * len, 70 - 20 - R2() * 40, TESLA[1] + Math.sin(ang2) * len];
        var prev = p0;
        for (var s2 = 1; s2 <= 7; s2++) {
          var f = s2 / 7, j = s2 < 7 ? 5 : 0;
          var pt = [p0[0] + (end[0] - p0[0]) * f + (R2() - 0.5) * j, p0[1] + (end[1] - p0[1]) * f + (R2() - 0.5) * j, p0[2] + (end[2] - p0[2]) * f + (R2() - 0.5) * j];
          arcs.push(prev[0], prev[1], prev[2], 1, pt[0], pt[1], pt[2], 1); prev = pt;
        }
      }
      heroTrail.unshift(hp); if (heroTrail.length > 30) heroTrail.length = 30;
      var trail = [];
      for (var k2 = 1; k2 < heroTrail.length; k2++) {
        var aa = 1 - k2 / heroTrail.length, p1 = heroTrail[k2 - 1], p2 = heroTrail[k2];
        if (k2 % 2) trail.push(p2[0], p2[1] - 0.6, p2[2], 0.15 + aa * 0.45);
      }
      var puffs = [];
      for (var sp = 0; sp < 26; sp++) {
        var life = ((time * 0.12 + sp / 26) % 1);
        var tower = sp % 2 ? [ATOM[0] - 26, ATOM[1]] : [ATOM[0] + 12, ATOM[1] - 22];
        puffs.push(tower[0] + Math.sin(sp * 3.1) * 4 + life * 30, 46 + life * 70, tower[1] + Math.cos(sp * 1.7) * 4, (1 - life) * 1.0 + 0.2);
      }
      if (rise > 0) {
        for (var sm = 0; sm < 40; sm++) {
          var tl = sm / 40 * Math.min(rise, 8), yy = 2 + Math.pow(Math.max(rise - tl, 0), 2) * 3.2;
          puffs.push(ROCKET[0] + Math.sin(sm * 2.3) * (2 + tl * 2), yy, ROCKET[1] + Math.cos(sm * 1.9) * (2 + tl * 2), 1.2 - tl * 0.08);
        }
      }
      gl.useProgram(lines.p); gl.uniformMatrix4fv(lines.u.u_vp, false, vp); gl.bindVertexArray(fxVao);
      gl.enable(gl.BLEND); gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
      gl.bindBuffer(gl.ARRAY_BUFFER, fx);
      gl.uniform1f(lines.u.u_points, 0);
      if (arcs.length) { gl.bufferSubData(gl.ARRAY_BUFFER, 0, new Float32Array(arcs)); gl.uniform4f(lines.u.u_col, 0.6, 1.0, 0.7, 1.0); gl.drawArrays(gl.LINES, 0, arcs.length / 4); }
      gl.uniform1f(lines.u.u_points, 1); gl.uniform1f(lines.u.u_size, 9000 * (h / 900));
      if (trail.length) { gl.bufferSubData(gl.ARRAY_BUFFER, 0, new Float32Array(trail)); gl.uniform4f(lines.u.u_col, 1.0, 0.78, 0.25, 1.0); gl.drawArrays(gl.POINTS, 0, trail.length / 4); }
      gl.bufferSubData(gl.ARRAY_BUFFER, 0, new Float32Array(puffs)); gl.uniform4f(lines.u.u_col, 0.98, 0.96, 0.92, 1.0);
      gl.drawArrays(gl.POINTS, 0, puffs.length / 4);
      gl.disable(gl.BLEND);
      gl.bindVertexArray(null);
    }

    function frame(now) {
      raf = 0;
      if (!visible) return;
      var dt = last ? Math.min((now - last) / 1000, 0.1) : 0;
      last = now;
      time += dt;
      // A slow graphics card: fewer pixels rather than stutter.
      if (dt > 0 && ++frames > 40) {
        slow = slow * 0.95 + (dt > 1 / 40 ? 1 : 0) * 0.05;
        if (slow > 0.6 && scale > 0.5) { scale *= 0.8; slow = 0; }
      }
      draw();
      if (frames === 3) canvas.classList.add("shown");
      raf = requestAnimationFrame(frame);
    }
    if (still) { draw(); canvas.classList.add("shown"); window.addEventListener("resize", draw); }
    else raf = requestAnimationFrame(frame);
    return true;
  }

  function invert(m) {
    var inv = new Array(16);
    inv[0] = m[5] * m[10] * m[15] - m[5] * m[11] * m[14] - m[9] * m[6] * m[15] + m[9] * m[7] * m[14] + m[13] * m[6] * m[11] - m[13] * m[7] * m[10];
    inv[4] = -m[4] * m[10] * m[15] + m[4] * m[11] * m[14] + m[8] * m[6] * m[15] - m[8] * m[7] * m[14] - m[12] * m[6] * m[11] + m[12] * m[7] * m[10];
    inv[8] = m[4] * m[9] * m[15] - m[4] * m[11] * m[13] - m[8] * m[5] * m[15] + m[8] * m[7] * m[13] + m[12] * m[5] * m[11] - m[12] * m[7] * m[9];
    inv[12] = -m[4] * m[9] * m[14] + m[4] * m[10] * m[13] + m[8] * m[5] * m[14] - m[8] * m[6] * m[13] - m[12] * m[5] * m[10] + m[12] * m[6] * m[9];
    inv[1] = -m[1] * m[10] * m[15] + m[1] * m[11] * m[14] + m[9] * m[2] * m[15] - m[9] * m[3] * m[14] - m[13] * m[2] * m[11] + m[13] * m[3] * m[10];
    inv[5] = m[0] * m[10] * m[15] - m[0] * m[11] * m[14] - m[8] * m[2] * m[15] + m[8] * m[3] * m[14] + m[12] * m[2] * m[11] - m[12] * m[3] * m[10];
    inv[9] = -m[0] * m[9] * m[15] + m[0] * m[11] * m[13] + m[8] * m[1] * m[15] - m[8] * m[3] * m[13] - m[12] * m[1] * m[11] + m[12] * m[3] * m[9];
    inv[13] = m[0] * m[9] * m[14] - m[0] * m[10] * m[13] - m[8] * m[1] * m[14] + m[8] * m[2] * m[13] + m[12] * m[1] * m[10] - m[12] * m[2] * m[9];
    inv[2] = m[1] * m[6] * m[15] - m[1] * m[7] * m[14] - m[5] * m[2] * m[15] + m[5] * m[3] * m[14] + m[13] * m[2] * m[7] - m[13] * m[3] * m[6];
    inv[6] = -m[0] * m[6] * m[15] + m[0] * m[7] * m[14] + m[4] * m[2] * m[15] - m[4] * m[3] * m[14] - m[12] * m[2] * m[7] + m[12] * m[3] * m[6];
    inv[10] = m[0] * m[5] * m[15] - m[0] * m[7] * m[13] - m[4] * m[1] * m[15] + m[4] * m[3] * m[13] + m[12] * m[1] * m[7] - m[12] * m[3] * m[5];
    inv[14] = -m[0] * m[5] * m[14] + m[0] * m[6] * m[13] + m[4] * m[1] * m[14] - m[4] * m[2] * m[13] - m[12] * m[1] * m[6] + m[12] * m[2] * m[5];
    inv[3] = -m[1] * m[6] * m[11] + m[1] * m[7] * m[10] + m[5] * m[2] * m[11] - m[5] * m[3] * m[10] - m[9] * m[2] * m[7] + m[9] * m[3] * m[6];
    inv[7] = m[0] * m[6] * m[11] - m[0] * m[7] * m[10] - m[4] * m[2] * m[11] + m[4] * m[3] * m[10] + m[8] * m[2] * m[7] - m[8] * m[3] * m[6];
    inv[11] = -m[0] * m[5] * m[11] + m[0] * m[7] * m[9] + m[4] * m[1] * m[11] - m[4] * m[3] * m[9] - m[8] * m[1] * m[7] + m[8] * m[3] * m[5];
    inv[15] = m[0] * m[5] * m[10] - m[0] * m[6] * m[9] - m[4] * m[1] * m[10] + m[4] * m[2] * m[9] + m[8] * m[1] * m[6] - m[8] * m[2] * m[5];
    var det = m[0] * inv[0] + m[1] * inv[4] + m[2] * inv[8] + m[3] * inv[12];
    det = det ? 1 / det : 0;
    for (var i = 0; i < 16; i++) inv[i] *= det;
    return inv;
  }

  window.HWScene = { start: start };
})();
