import base64
import json
import os
import re
import sys

DOCK = sys.argv[1]
REPO = sys.argv[2]
OUT = sys.argv[3]

core = open(os.path.join(REPO, "crates/drifter-core/src/shader.rs")).read()
head = re.search(r'pub const HEAD: &str = "(.*?)";\n', core, re.S).group(1)
aurora = open(os.path.join(REPO, "shaders/aurora.shader")).read()
keys = []
for m in re.finditer(r"\(([\d.]+), \[(0x[0-9a-f]+), (0x[0-9a-f]+), (0x[0-9a-f]+), (0x[0-9a-f]+), (0x[0-9a-f]+)\], ([\d.]+)\)", core):
    keys.append([float(m.group(1)), [int(m.group(i), 16) for i in range(2, 7)], float(m.group(7))])
assert len(keys) == 11, len(keys)


def b64(path):
    return "data:image/png;base64," + base64.b64encode(open(path, "rb").read()).decode()


APPS = [("apps", "Apps", False), ("files", "Files", True), ("terminal", "Terminal", True), ("browser", "Browser", True),
        ("mail", "Mail", False), ("music", "Music", True), ("photos", "Photos", False), ("calendar", "Calendar", False),
        ("spotify", "Spotify", False)]
TAIL = [("settings", "Settings", False), ("trash", "Trash", False)]
icons = {n: {"c": b64(os.path.join(DOCK, "amb", f"{n}.png")), "n": b64(os.path.join(DOCK, "nrm", f"{n}.png"))} for n, _, _ in APPS + TAIL}

page = r"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Drifter Dock Light</title>
<style>
@font-face { font-family: "Work Sans"; src: url("../../fonts/WorkSans.ttf"); font-weight: 100 900; }
:root { --ink: #eceef4; --dim: #9298a6; --panel: rgba(16,18,26,.62); --rim: rgba(255,255,255,.12); }
* { box-sizing: border-box; }
html, body { margin: 0; height: 100%; overflow: hidden; background: #04060f; color: var(--ink); font-family: "Work Sans", system-ui, sans-serif; }
#wall { position: fixed; inset: 0; width: 100%; height: 100%; display: block; }
#bar { position: fixed; left: 0; right: 0; bottom: 0; backdrop-filter: blur(22px) saturate(1.5); -webkit-backdrop-filter: blur(22px) saturate(1.5); border-top: 1px solid var(--rim); transition: background .6s; }
#lit { position: fixed; left: 0; bottom: 0; width: 100%; pointer-events: none; }
#hit { position: fixed; left: 0; right: 0; bottom: 0; }
.panel {
  position: fixed; top: 16px; left: 50%; transform: translateX(-50%); width: min(900px, calc(100% - 32px));
  background: var(--panel); border: 1px solid var(--rim); border-radius: 16px; padding: 14px 16px 12px;
  backdrop-filter: blur(24px) saturate(1.4); -webkit-backdrop-filter: blur(24px) saturate(1.4);
  box-shadow: 0 10px 40px rgba(0,0,0,.25);
}
.line { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; }
.clock { font-size: 30px; font-weight: 600; font-variant-numeric: tabular-nums; min-width: 92px; }
.what { color: var(--dim); font-size: 13px; min-width: 120px; }
input[type=range] { flex: 1; min-width: 200px; accent-color: #3584e4; }
.btns { margin-top: 10px; display: flex; gap: 6px; flex-wrap: wrap; }
button {
  font: inherit; font-size: 13px; color: var(--ink); background: rgba(255,255,255,.08); border: 1px solid var(--rim);
  border-radius: 8px; padding: 6px 11px; cursor: pointer;
}
button:hover { background: rgba(255,255,255,.14); }
button.on { background: #3584e4; border-color: #3584e4; color: #fff; }
.sep { width: 1px; background: var(--rim); margin: 0 4px; }
.tip { position: fixed; padding: 4px 9px; border-radius: 7px; font-size: 12.5px; background: var(--panel); border: 1px solid var(--rim); backdrop-filter: blur(16px); pointer-events: none; opacity: 0; transition: opacity .12s; transform: translateX(-50%); }
@media (max-width: 640px) { .clock { font-size: 24px; min-width: 76px; } }
</style>
</head>
<body>
<canvas id="wall"></canvas>
<div id="bar"></div>
<canvas id="lit"></canvas>
<div id="hit"></div>
<div class="tip" id="tip"></div>
<div class="panel">
  <div class="line">
    <span class="clock" id="clock">12:00</span>
    <span class="what" id="what">Noon</span>
    <input type="range" id="hour" min="0" max="24" step="0.01" value="12">
  </div>
  <div class="btns">
    <button id="play">Play day</button>
    <button id="now">Now</button>
    <span class="sep"></span>
    <button data-h="5.9">Dawn</button>
    <button data-h="8.5">Morning</button>
    <button data-h="13">Noon</button>
    <button data-h="17.2">Afternoon</button>
    <button data-h="19.1">Sunset</button>
    <button data-h="23.5">Night</button>
    <span class="sep"></span>
    <button data-size="52">Small</button>
    <button data-size="64" class="on">Medium</button>
    <button data-size="84">Large</button>
    <span class="sep"></span>
    <button id="live" class="on">Live light</button>
  </div>
</div>
<script>
const KEYS = __KEYS__;
const ICONS = __ICONS__;
const APPS = __APPS__;
const TAIL = __TAIL__;
const HEAD = __HEAD__;
const AURORA = __AURORA__;

const rgb = (h) => [(h >> 16 & 255) / 255, (h >> 8 & 255) / 255, (h & 255) / 255];
const ss = (a, b, x) => { const t = Math.min(Math.max((x - a) / (b - a), 0), 1); return t * t * (3 - 2 * t); };
const mix = (a, b, t) => a + (b - a) * t;
const mix3 = (a, b, t) => a.map((v, i) => mix(v, b[i], t));
function sky(hour) {
  hour = ((hour % 24) + 24) % 24;
  let at = KEYS.findIndex((k, i) => i < KEYS.length - 1 && hour < KEYS[i + 1][0]);
  if (at < 0) at = KEYS.length - 2;
  const from = KEYS[at], to = KEYS[at + 1];
  const f = ss(0, 1, (hour - from[0]) / (to[0] - from[0]));
  const c = (i) => mix3(rgb(from[1][i]), rgb(to[1][i]), f);
  return { top: c(0), bottom: c(1), a: c(2), b: c(3), c: c(4), light: mix(from[2], to[2], f) };
}

const RISE = 6.0, SET = 19.5;
function lights(hour) {
  const s = sky(hour);
  const day = (hour - RISE) / (SET - RISE);
  const night = (((hour - SET) % 24) + 24) % 24 / (24 - (SET - RISE));
  const sunUp = Math.sin(Math.PI * day);
  const moonUp = night <= 1 ? Math.sin(Math.PI * night) : -1;
  const arc = (t, up) => {
    const x = -Math.cos(Math.PI * t);
    const e = Math.max(up, 0);
    const v = [x * 0.85, 0.25 + 0.35 * e, 0.3 + 0.9 * e];
    const n = Math.hypot(...v);
    return v.map(c => c / n);
  };
  const warm = Math.pow(1 - Math.max(sunUp, 0), 2);
  const sun = {
    dir: arc(day, sunUp),
    color: mix3([1, 0.97, 0.92], mix3(s.b, [1, 0.6, 0.35], 0.3), warm * 0.85),
    power: (day > -0.05 && day < 1.05 ? ss(-0.1, 0.12, sunUp) : 0) * mix(0.85, 0.62, Math.max(sunUp, 0)),
    up: sunUp,
  };
  const moon = {
    dir: arc(night, moonUp),
    color: [0.66, 0.75, 1.0],
    power: ss(-0.1, 0.12, moonUp) * 0.5,
    up: moonUp,
  };
  const mid = mix3(s.top, s.bottom, 0.5);
  const amb = mix3([1, 1, 1], mix3(mid, s.a, 0.25), 0.22).map(c => c * mix(0.62, 0.55, s.light));
  return { s, sun, moon, amb };
}

function name(hour) {
  const s = sky(hour).light;
  if (hour >= 5 && hour < 7) return "Dawn";
  if (hour >= 7 && hour < 11) return "Morning";
  if (hour >= 11 && hour < 15) return "Midday";
  if (hour >= 15 && hour < 18.3) return "Afternoon";
  if (hour >= 18.3 && hour < 20.5) return "Sunset";
  return s < 0.05 ? "Night (moonlight)" : "Dusk";
}

function program(gl, vs, fs) {
  const make = (type, src) => {
    const sh = gl.createShader(type); gl.shaderSource(sh, src); gl.compileShader(sh);
    if (!gl.getShaderParameter(sh, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(sh));
    return sh;
  };
  const p = gl.createProgram();
  gl.attachShader(p, make(gl.VERTEX_SHADER, vs)); gl.attachShader(p, make(gl.FRAGMENT_SHADER, fs));
  gl.linkProgram(p);
  if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(p));
  return p;
}
const QUAD_VS = "attribute vec2 p; uniform vec4 box; varying vec2 v_coords; void main(){ v_coords = p; vec2 q = box.xy + p * box.zw; gl_Position = vec4(q.x * 2. - 1., 1. - q.y * 2., 0., 1.); }";

const wallCanvas = document.getElementById("wall");
const wg = wallCanvas.getContext("webgl", { premultipliedAlpha: true, antialias: false });
const wallFoot = "\nvoid main(){\n vec2 P=vec2(v_coords.x,1.-v_coords.y)*R;\n vec3 c=wall(P)+(h(P)-.5)/255.;\n gl_FragColor=vec4(clamp(c,0.,1.),1.)*alpha;\n}\n";
const wp = program(wg, QUAD_VS, HEAD + AURORA + wallFoot);
const quad = (gl) => { const b = gl.createBuffer(); gl.bindBuffer(gl.ARRAY_BUFFER, b); gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([0,0, 1,0, 0,1, 1,1]), gl.STATIC_DRAW); return b; };
const wq = quad(wg);

const ICON_FS = `
precision highp float;
varying vec2 v_coords;
uniform sampler2D col; uniform sampler2D nrm;
uniform float pad; uniform float live;
uniform vec3 sunDir; uniform vec3 sunCol; uniform float sunPow; uniform float sunUp;
uniform vec3 moonDir; uniform vec3 moonCol; uniform float moonPow; uniform float moonUp;
uniform vec3 amb; uniform vec3 shadeTint;
vec2 icon(vec2 uv){ return uv * (1. + 2. * pad) - pad; }
float inside(vec2 q){ return step(0., q.x) * step(0., q.y) * step(q.x, 1.) * step(q.y, 1.); }
float alphaAt(vec2 q){ return texture2D(col, q).a * inside(q); }
float shadow(vec2 q, vec3 dir, float up){
  float low = 1. - clamp(up, 0., 1.);
  vec2 away = normalize(vec2(-dir.x, dir.y) + vec2(0., 0.0001));
  vec2 off = away * (0.035 + 0.15 * low) + vec2(0., 0.025);
  float r = 0.06 + 0.07 * low;
  float s = 0.;
  for (int i = 0; i < 32; i++) {
    float a = float(i) * 2.39996;
    float d = sqrt((float(i) + .5) / 32.);
    s += alphaAt(q - off + vec2(cos(a), sin(a)) * d * r);
  }
  return s / 32.;
}
void main(){
  vec2 q = icon(v_coords);
  vec4 c = texture2D(col, q) * inside(q);
  vec3 n = texture2D(nrm, q).xyz * 2. - 1.;
  n = normalize(vec3(n.x, n.y, max(n.z, 0.05)));
  vec3 base = c.rgb;
  vec3 albedo = base;
  float sd = max(dot(n, sunDir), 0.), md = max(dot(n, moonDir), 0.);
  vec3 v = vec3(0., 0., 1.);
  float sp = pow(max(dot(reflect(-sunDir, n), v), 0.), 22.) * 0.22 * sunPow;
  float mp = pow(max(dot(reflect(-moonDir, n), v), 0.), 26.) * 0.2 * moonPow;
  vec3 lit = albedo * (amb + sunCol * sunPow * sd + moonCol * moonPow * md) + sunCol * sp + moonCol * mp;
  float rim = pow(1. - n.z, 2.) * 0.25;
  lit += (sunCol * sunPow * max(dot(n.xy, sunDir.xy), 0.) + moonCol * moonPow * max(dot(n.xy, moonDir.xy), 0.)) * rim;
  vec3 color = mix(base, lit, live);
  float sh = shadow(q, sunDir, sunUp) * sunPow * 0.42 + shadow(q, moonDir, moonUp) * moonPow * 0.5;
  float still = shadow(q, vec3(0., 1., 1.), 1.) * 0.24;
  sh = clamp(mix(still, sh, live), 0., 0.5);
  float a = c.a;
  vec3 outc = color * a + shadeTint * sh * (1. - a);
  gl_FragColor = vec4(outc, a + sh * (1. - a));
}`;
const litCanvas = document.getElementById("lit");
const lg = litCanvas.getContext("webgl", { premultipliedAlpha: true, alpha: true });
const lp = program(lg, QUAD_VS, ICON_FS);
const lq = quad(lg);
const textures = {};
function texture(gl, src) {
  return new Promise((done) => {
    const img = new Image();
    img.onload = () => {
      const t = gl.createTexture();
      gl.bindTexture(gl.TEXTURE_2D, t);
      gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL, false);
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, img);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
      done(t);
    };
    img.src = src;
  });
}

let hour = 12, playing = false, live = 1, size = 64, last = performance.now(), start = performance.now();
let pointer = { x: -1, y: -1 };
const hover = {};
const slider = document.getElementById("hour");
const clock = document.getElementById("clock");
const what = document.getElementById("what");
const bar = document.getElementById("bar");
const hit = document.getElementById("hit");
const tip = document.getElementById("tip");

function layout() {
  const dpr = window.devicePixelRatio || 1;
  const W = innerWidth, H = innerHeight;
  const padY = Math.round(size * 0.12), barH = size + padY * 2 + 6;
  bar.style.height = barH + "px";
  hit.style.height = barH + "px";
  const extra = Math.round(size * 0.9);
  litCanvas.style.height = (barH + extra) + "px";
  for (const [c, w, h] of [[wallCanvas, W, H], [litCanvas, W, barH + extra]]) {
    c.width = Math.round(w * dpr); c.height = Math.round(h * dpr);
  }
  const gap = Math.round(size * 0.18), divider = Math.round(size * 0.35);
  const all = APPS.length + TAIL.length;
  const width = all * size + (all - 1) * gap + divider;
  let x = (W - width) / 2;
  const slots = [];
  APPS.forEach((a) => { slots.push({ key: a[0], label: a[1], running: a[2], x, y: extra + padY }); x += size + gap; });
  const rule = x - gap + (divider + gap) / 2;
  x += divider;
  TAIL.forEach((a) => { slots.push({ key: a[0], label: a[1], running: a[2], x, y: extra + padY }); x += size + gap; });
  return { W, H, barH, extra, slots, rule, dpr };
}
let L = layout();
addEventListener("resize", () => { L = layout(); });

function setHour(h, fromSlider) {
  hour = ((h % 24) + 24) % 24;
  if (!fromSlider) slider.value = hour;
  const hh = Math.floor(hour), mm = Math.floor((hour - hh) * 60);
  clock.textContent = String(hh).padStart(2, "0") + ":" + String(mm).padStart(2, "0");
  what.textContent = name(hour);
}
const nowHour = () => { const d = new Date(); return d.getHours() + d.getMinutes() / 60 + d.getSeconds() / 3600; };
slider.oninput = () => { playing = false; document.getElementById("play").classList.remove("on"); document.getElementById("play").textContent = "Play day"; setHour(parseFloat(slider.value), true); };
document.getElementById("play").onclick = (e) => { playing = !playing; e.target.classList.toggle("on", playing); e.target.textContent = playing ? "Pause" : "Play day"; };
document.getElementById("now").onclick = () => setHour(nowHour());
document.getElementById("live").onclick = (e) => { live = live ? 0 : 1; e.target.classList.toggle("on", !!live); };
document.querySelectorAll("[data-h]").forEach(b => b.onclick = () => setHour(parseFloat(b.dataset.h)));
document.querySelectorAll("[data-size]").forEach(b => b.onclick = () => {
  size = parseInt(b.dataset.size, 10);
  document.querySelectorAll("[data-size]").forEach(o => o.classList.toggle("on", o === b));
  L = layout();
});
addEventListener("pointermove", (e) => { pointer = { x: e.clientX, y: e.clientY }; });
addEventListener("pointerleave", () => { pointer = { x: -1, y: -1 }; });

function drawWall(t, lt) {
  const gl = wg;
  gl.viewport(0, 0, wallCanvas.width, wallCanvas.height);
  gl.useProgram(wp);
  const u = (n) => gl.getUniformLocation(wp, n);
  gl.uniform4f(u("box"), 0, 0, 1, 1);
  gl.uniform2f(u("W"), wallCanvas.width, wallCanvas.height);
  gl.uniform2f(u("size"), wallCanvas.width, wallCanvas.height);
  gl.uniform1f(u("alpha"), 1);
  gl.uniform1f(u("T"), t);
  gl.uniform1f(u("L"), lt.s.light);
  gl.uniform3fv(u("S0"), lt.s.top); gl.uniform3fv(u("S1"), lt.s.bottom);
  gl.uniform3fv(u("A"), lt.s.a); gl.uniform3fv(u("B"), lt.s.b); gl.uniform3fv(u("C"), lt.s.c);
  gl.uniform3f(u("M"), pointer.x * L.dpr, (L.H - pointer.y) * L.dpr, 0);
  gl.bindBuffer(gl.ARRAY_BUFFER, wq);
  const at = gl.getAttribLocation(wp, "p");
  gl.enableVertexAttribArray(at); gl.vertexAttribPointer(at, 2, gl.FLOAT, false, 0, 0);
  gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
}

function drawDock(lt, dt) {
  const gl = lg;
  const cw = litCanvas.width, ch = litCanvas.height, dpr = L.dpr;
  gl.viewport(0, 0, cw, ch);
  gl.clearColor(0, 0, 0, 0); gl.clear(gl.COLOR_BUFFER_BIT);
  gl.enable(gl.BLEND); gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
  gl.useProgram(lp);
  const u = (n) => gl.getUniformLocation(lp, n);
  gl.bindBuffer(gl.ARRAY_BUFFER, lq);
  const at = gl.getAttribLocation(lp, "p");
  gl.enableVertexAttribArray(at); gl.vertexAttribPointer(at, 2, gl.FLOAT, false, 0, 0);
  gl.uniform1f(u("live"), live);
  gl.uniform3fv(u("sunDir"), lt.sun.dir); gl.uniform3fv(u("sunCol"), lt.sun.color); gl.uniform1f(u("sunPow"), lt.sun.power); gl.uniform1f(u("sunUp"), lt.sun.up);
  gl.uniform3fv(u("moonDir"), lt.moon.dir); gl.uniform3fv(u("moonCol"), lt.moon.color); gl.uniform1f(u("moonPow"), lt.moon.power); gl.uniform1f(u("moonUp"), lt.moon.up);
  gl.uniform3fv(u("amb"), live ? lt.amb : [1, 1, 1]);
  gl.uniform3fv(u("shadeTint"), mix3([0.01, 0.015, 0.05], [0.02, 0.03, 0.12], 1 - lt.s.light));
  gl.uniform1i(u("col"), 0); gl.uniform1i(u("nrm"), 1);
  const pad = 0.42;
  gl.uniform1f(u("pad"), pad);
  const top = innerHeight - (L.barH + L.extra);
  let over = null;
  for (const s of L.slots) {
    const t = textures[s.key]; if (!t) continue;
    const inX = pointer.x >= s.x && pointer.x <= s.x + size;
    const inY = pointer.y >= top + s.y - 4 && pointer.y <= innerHeight;
    const goal = inX && inY ? 1 : 0;
    if (goal) over = s;
    hover[s.key] = (hover[s.key] || 0) + (goal - (hover[s.key] || 0)) * Math.min(dt * 14, 1);
    const lift = -hover[s.key] * size * 0.07;
    const big = size * (1 + 2 * pad);
    const x = s.x - size * pad, y = s.y - size * pad + lift;
    gl.uniform4f(u("box"), x / L.W, y / (ch / dpr), big / L.W, big / (ch / dpr));
    gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D, t.c);
    gl.activeTexture(gl.TEXTURE1); gl.bindTexture(gl.TEXTURE_2D, t.n);
    gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
  }
  if (over) {
    tip.style.opacity = 1; tip.textContent = over.label;
    tip.style.left = (over.x + size / 2) + "px"; tip.style.top = (top + over.y - 34) + "px";
  } else tip.style.opacity = 0;
}

const dots = document.createElement("div");
document.body.appendChild(dots);
function drawExtras(lt) {
  const light = lt.s.light >= 0.3;
  document.documentElement.style.setProperty("--panel", light ? "rgba(248,248,250,.62)" : "rgba(16,18,26,.62)");
  document.documentElement.style.setProperty("--ink", light ? "#1a1c22" : "#eceef4");
  document.documentElement.style.setProperty("--dim", light ? "#646a78" : "#9298a6");
  document.documentElement.style.setProperty("--rim", light ? "rgba(0,0,0,.10)" : "rgba(255,255,255,.12)");
  bar.style.background = light ? "rgba(238,239,243,.55)" : "rgba(16,18,26,.5)";
  const top = innerHeight - L.barH;
  let html = `<div style="position:fixed;left:${L.rule}px;top:${top + L.barH * 0.3}px;width:1px;height:${L.barH * 0.4}px;background:${light ? "rgba(0,0,0,.18)" : "rgba(255,255,255,.2)"}"></div>`;
  for (const s of L.slots) if (s.running) {
    const d = Math.max(4, size * 0.07);
    html += `<div style="position:fixed;left:${s.x + size / 2 - d / 2}px;top:${innerHeight - d - 3}px;width:${d}px;height:${d}px;border-radius:50%;background:${light ? "#1a1c22" : "#eceef4"};opacity:.75"></div>`;
  }
  if (dots.innerHTML !== html) dots.innerHTML = html;
}

function frame(now) {
  const dt = Math.min((now - last) / 1000, 0.1); last = now;
  if (playing) setHour(hour + dt * 24 / 40);
  const lt = lights(hour);
  drawWall((now - start) / 1000, lt);
  drawDock(lt, dt);
  drawExtras(lt);
  requestAnimationFrame(frame);
}

Promise.all([...APPS, ...TAIL].map(async (a) => {
  textures[a[0]] = { c: await texture(lg, ICONS[a[0]].c), n: await texture(lg, ICONS[a[0]].n) };
})).then(() => { setHour(nowHour()); requestAnimationFrame(frame); });
</script>
</body>
</html>
"""

page = (page.replace("__KEYS__", json.dumps(keys)).replace("__ICONS__", json.dumps(icons))
        .replace("__APPS__", json.dumps(APPS)).replace("__TAIL__", json.dumps(TAIL))
        .replace("__HEAD__", json.dumps(head.encode().decode("unicode_escape") if "\\n" in head else head))
        .replace("__AURORA__", json.dumps(aurora)))
with open(OUT, "w") as f:
    f.write(page)
print("ok", len(page))
