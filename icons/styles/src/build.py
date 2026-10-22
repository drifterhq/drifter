import html
import json
import os
import sys

SRC = sys.argv[1]
OUT = sys.argv[2]
SUBJECTS = json.load(open(os.path.join(SRC, "subjects.json")))
ORDER = ["files", "terminal", "browser", "settings", "mail", "music", "photos", "calendar", "trash", "activity", "document", "archive", "spotify"]
DOCK = ["files", "browser", "terminal", "mail", "music", "photos", "settings", "trash"]

STYLES = [
    ("glass", "Glass Drop", "Clear glass with no tile. The shape is the icon. The aurora bends through it, like the Drifter drop logo.", "Glass, refraction, aurora"),
    ("panes", "Frosted Panes", "A color slab with a frosted pane floating in front of it, offset. The glyph sits on the frost. It uses the same idea as Drifter's frosted panels.", "Frost, layers, depth"),
    ("clay", "Soft Clay", "Thick matte forms with soft light. It is calm and friendly, and the shapes are easy to read at small sizes.", "Matte, puffy, quiet"),
    ("enamel", "Enamel Pin", "Glossy enamel color inside raised silver lines, cut to the shape. It feels like a badge you could pin on a bag.", "Metal, gloss, crafted"),
    ("neon", "Night Neon", "Glowing tubes that trace the outline. It is made for the dark desktop and the aurora wallpaper.", "Light, outline, night"),
    ("iso", "Isometric Block", "Thick blocks seen from above at an angle, like objects on a map. It fits Drifter's idea of the desktop as a place you look down on.", "Geography, solid, map"),
    ("topo", "Topo Strata", "The shape is built from stacked contour layers, like a terrain map, from dark at the base to light at the top. It matches the Topo wallpaper.", "Contours, terrain, layers"),
    ("smoked", "Smoked Glass", "Dark tinted glass with light in its edges, like edge-lit acrylic. The glyph glows from inside. It is the darkest style here.", "Dark, edge light, glow"),
    ("porcelain", "Porcelain Glaze", "White glossy ceramic with an inlay of colored glaze and a thin white rim. It is clean and works best on the light theme.", "Ceramic, white, glaze"),
    ("line", "Aurora Line", "Flat. Thin rounded lines with a soft tint fill and a slight glow. It is the one 2D style, for people who want less depth.", "Flat, line, light"),
]


def line_svg(name):
    s = SUBJECTS[name]
    hue = s["hue"]
    gid = f"g-{name}"
    parts = []
    for p in s["parts"]:
        if p["kind"] == "line":
            parts.append(f'<path d="{p["d"]}" fill="none" stroke="url(#{gid})" stroke-width="{max(p["width"] * 0.75, 3.6)}" stroke-linecap="round" stroke-linejoin="round"/>')
        else:
            fill = f'{hue}' if p["role"] == "body" else "none"
            parts.append(
                f'<path d="{p["d"]}" fill="{fill}" fill-opacity="0.14" stroke="url(#{gid})" stroke-width="3.6" '
                f'stroke-linecap="round" stroke-linejoin="round" fill-rule="evenodd"/>'
            )
    inner = "".join(parts)
    t = s.get("transform")
    if t:
        inner = inner.replace('stroke-width="3.6"', 'stroke-width="1.14"')
        inner = f'<g transform="{t}">{inner}</g>'
    return (
        f'<svg viewBox="-6 -6 112 112" class="line" style="--hue:{hue}">'
        f'<defs><linearGradient id="{gid}" x1="0" y1="0" x2="1" y2="1">'
        f'<stop offset="0" stop-color="#c8fff0"/><stop offset="0.45" stop-color="{hue}"/><stop offset="1" stop-color="#8f7bff"/>'
        f'</linearGradient></defs>{inner}</svg>'
    )


def icon(style, name):
    label = html.escape(SUBJECTS[name]["label"])
    if style == "line":
        art = line_svg(name)
    else:
        art = f'<img src="img/{style}/{name}.png" alt="{label}">'
    return f'<figure class="icon"><div class="art">{art}</div><figcaption>{label}</figcaption></figure>'


def dock(style):
    items = []
    for name in DOCK:
        if style == "line":
            items.append(f'<div class="slot">{line_svg(name).replace(f"g-{name}", f"d-{name}")}</div>')
        else:
            items.append(f'<div class="slot"><img src="img/{style}/{name}.png" alt=""></div>')
    return f'<div class="dock">{"".join(items)}</div>'


sections = []
LIST = [
    ("files", "Projects", "Folder", "—"), ("files", "Pictures", "Folder", "—"), ("document", "notes.md", "Text", "4 KB"),
    ("archive", "drifter-0.9.tar.gz", "Archive", "18.2 MB"), ("photos", "aurora.png", "Image", "6.1 MB"),
    ("music", "night-drive.flac", "Audio", "32.4 MB"), ("terminal", "build.sh", "Script", "2 KB"),
    ("calendar", "trip.ics", "Calendar", "1 KB"), ("mail", "invite.eml", "Mail", "12 KB"),
]
def sizes(folder):
    return "".join(
        f'<div class="sz"><img src="img/{folder}/{n}.png" alt="" style="width:{px}px;height:{px}px"></div>'
        for px in (16, 24, 32, 48) for n in ("files", "document", "terminal", "settings", "photos")
    )
rows = "".join(
    f'<div class="lrow"><img src="img/clay-flat/{n}.png" alt=""><span>{html.escape(f)}</span><span>{k}</span><span>{z}</span></div>'
    for n, f, k, z in LIST
)
big = "".join(icon("clay", n) for n in ORDER)
flat = "".join(icon("clay-flat", n) for n in ORDER)
sections.append(
    '<section class="style chosen" id="chosen"><header><span class="num">Pick</span><div><h2>Soft Clay, two views</h2>'
    '<p>Tilted for large sizes: dock, Launchpad, icon view. Straight-on for small sizes: list view, menus, sidebars.</p></div></header>'
    f'<h3>Tilted (large)</h3><div class="row">{big}</div>'
    f'<h3>Straight-on (small)</h3><div class="row">{flat}</div>'
    '<div class="pair"><div><h3>Small sizes: tilted</h3>'
    f'<div class="sizes">{sizes("clay")}</div></div><div><h3>Small sizes: straight-on</h3><div class="sizes">{sizes("clay-flat")}</div></div></div>'
    '<h3>Explorer list view, straight-on at 20 px</h3>'
    f'<div class="list"><div class="lrow head"><span></span><span>Name</span><span>Kind</span><span>Size</span></div>{rows}</div>'
    '</section>'
)
now = "".join(
    f'<figure class="icon"><div class="art"><img src="img/now/{n}.png" alt=""></div><figcaption>{html.escape(SUBJECTS[n]["label"])}</figcaption></figure>'
    for n in ORDER
)
sections.append(
    '<section class="style now"><header><span class="num">Now</span><div><h2>Squircle tiles</h2>'
    '<p>Today\'s set, for comparison. Each icon sits on the navy aurora squircle.</p></div></header>'
    f'<div class="row">{now}</div></section>'
)
for i, (key, title, text, tags) in enumerate(STYLES, 1):
    row = "".join(icon(key, n) for n in ORDER)
    sections.append(
        f'<section class="style" id="{key}"><header><span class="num">{i:02d}</span><div><h2>{title}</h2><p>{text}</p>'
        f'<span class="tags">{tags}</span></div></header><div class="row">{row}</div>{dock(key)}</section>'
    )

nav = '<a href="#chosen"><b>Pick</b> Soft Clay</a>' + "".join(f'<a href="#{k}"><b>{i:02d}</b> {t}</a>' for i, (k, t, _, _) in enumerate(STYLES, 1))

page = f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Drifter Icon Styles</title>
<style>
@font-face {{ font-family: "Work Sans"; src: url("../../fonts/WorkSans.ttf"); font-weight: 100 900; }}
:root {{
  --bar: #10121a; --back: #1e212a; --card: #2a2d39; --raised: #343948; --rule: #3a3e4e;
  --ink: #eceef4; --dim: #9298a6; --a: #3dffb0; --b: #7a4dff; --size: 96px;
  --sky0: #0a0f24; --sky1: #151b3a; --glow-a: rgba(61,255,176,.16); --glow-b: rgba(122,77,255,.22);
  --frost: rgba(42,45,57,.55); --frost-rim: rgba(255,255,255,.10);
}}
:root[data-theme="light"] {{
  --bar: #eeeff3; --back: #f8f8fa; --card: #ffffff; --raised: #e2e5ec; --rule: #d8dbe2;
  --ink: #1a1c22; --dim: #646a78; --sky0: #dfe5f2; --sky1: #f3f1fa;
  --glow-a: rgba(61,200,150,.20); --glow-b: rgba(122,77,255,.16);
  --frost: rgba(255,255,255,.6); --frost-rim: rgba(0,0,0,.08);
}}
* {{ box-sizing: border-box; }}
html {{ scroll-behavior: smooth; }}
body {{
  margin: 0; color: var(--ink); font-family: "Work Sans", system-ui, sans-serif;
  background:
    radial-gradient(1200px 500px at 15% -5%, var(--glow-a), transparent 70%),
    radial-gradient(1000px 600px at 90% 10%, var(--glow-b), transparent 70%),
    linear-gradient(180deg, var(--sky1), var(--sky0) 60%, var(--sky1));
  min-height: 100vh;
}}
.top {{
  position: sticky; top: 0; z-index: 5; display: flex; gap: 16px; align-items: center; flex-wrap: wrap;
  padding: 12px 24px; background: var(--frost); backdrop-filter: blur(24px) saturate(1.4);
  border-bottom: 1px solid var(--frost-rim);
}}
.top h1 {{ font-size: 17px; font-weight: 600; margin: 0 12px 0 0; letter-spacing: .01em; }}
.seg {{ display: inline-flex; background: var(--raised); border-radius: 9px; padding: 3px; }}
.seg button {{
  font: inherit; font-size: 13px; color: var(--dim); background: none; border: 0; padding: 5px 12px;
  border-radius: 7px; cursor: pointer;
}}
.seg button.on {{ background: var(--card); color: var(--ink); box-shadow: 0 1px 3px rgba(0,0,0,.25); }}
.top label {{ font-size: 13px; color: var(--dim); display: flex; gap: 8px; align-items: center; }}
input[type=range] {{ accent-color: #3584e4; width: 140px; }}
nav {{ display: flex; flex-wrap: wrap; gap: 6px; padding: 18px 24px 0; }}
nav a {{
  font-size: 13px; color: var(--dim); text-decoration: none; padding: 6px 11px; border-radius: 8px;
  background: var(--frost); border: 1px solid var(--frost-rim);
}}
nav a:hover {{ color: var(--ink); }}
nav b {{ color: var(--ink); font-weight: 600; margin-right: 3px; }}
main {{ padding: 8px 24px 80px; max-width: 1800px; margin: 0 auto; }}
.intro {{ color: var(--dim); font-size: 15px; max-width: 760px; line-height: 1.5; margin: 18px 0 6px; }}
.style {{
  margin-top: 22px; padding: 22px 22px 18px; border-radius: 18px; background: var(--frost);
  border: 1px solid var(--frost-rim); backdrop-filter: blur(18px);
  box-shadow: 0 10px 40px rgba(0,0,0,.18);
}}
.style header {{ display: flex; gap: 18px; align-items: flex-start; margin-bottom: 10px; }}
.num {{
  font-size: 13px; font-weight: 600; color: var(--dim); background: var(--raised); border-radius: 8px;
  padding: 5px 9px; min-width: 42px; text-align: center; margin-top: 2px;
}}
.style h2 {{ margin: 0; font-size: 22px; font-weight: 600; }}
.style p {{ margin: 4px 0 0; color: var(--dim); font-size: 14.5px; line-height: 1.45; max-width: 820px; }}
.tags {{ display: inline-block; margin-top: 8px; font-size: 12px; color: var(--dim); letter-spacing: .04em; text-transform: uppercase; }}
.row {{ display: flex; flex-wrap: wrap; gap: 6px 4px; }}
.icon {{ margin: 0; width: calc(var(--size) + 12px); text-align: center; }}
.art {{ width: var(--size); height: var(--size); margin: 0 auto; display: grid; place-items: center; }}
.art img, .art svg {{ width: 100%; height: 100%; display: block; }}
.art svg.line {{ width: 86%; height: 86%; filter: drop-shadow(0 0 5px color-mix(in srgb, var(--hue) 55%, transparent)); }}
figcaption {{ font-size: 12px; color: var(--dim); margin-top: 2px; }}
.dock {{
  display: inline-flex; gap: 6px; align-items: end; margin-top: 16px; padding: 7px 10px; border-radius: 20px;
  background: var(--frost); border: 1px solid var(--frost-rim); backdrop-filter: blur(20px);
}}
.dock .slot {{ width: 58px; height: 58px; display: grid; place-items: center; transition: transform .15s; }}
.dock .slot:hover {{ transform: translateY(-4px); }}
.dock img, .dock svg {{ width: 100%; height: 100%; }}
.dock svg.line {{ width: 80%; height: 80%; filter: drop-shadow(0 0 4px color-mix(in srgb, var(--hue) 55%, transparent)); }}
.style h3 {{ font-size: 13px; font-weight: 600; color: var(--dim); text-transform: uppercase; letter-spacing: .05em; margin: 18px 0 8px; }}
.pair {{ display: flex; gap: 40px; flex-wrap: wrap; }}
.sizes {{ display: grid; grid-template-columns: repeat(5, 56px); gap: 6px; align-items: center; }}
.sz {{ display: grid; place-items: center; height: 52px; }}
.list {{ max-width: 620px; background: var(--card); border-radius: 12px; padding: 6px; border: 1px solid var(--frost-rim); }}
.lrow {{ display: grid; grid-template-columns: 28px 1fr 110px 80px; align-items: center; height: 28px; padding: 0 8px; border-radius: 6px; font-size: 13.5px; }}
.lrow:nth-child(even) {{ background: color-mix(in srgb, var(--raised) 45%, transparent); }}
.lrow img {{ width: 20px; height: 20px; }}
.lrow span:nth-child(3), .lrow span:nth-child(4) {{ color: var(--dim); }}
.lrow.head {{ font-size: 12px; color: var(--dim); background: none !important; }}
.now .art img {{ width: 84%; height: 84%; }}
@media (max-width: 640px) {{ main, nav, .top {{ padding-left: 16px; padding-right: 16px; }} }}
</style>
</head>
<body>
<div class="top">
  <h1>Drifter icon styles</h1>
  <div class="seg" id="theme"><button data-v="dark" class="on">Dark</button><button data-v="light">Light</button></div>
  <label>Size <input type="range" id="size" min="48" max="192" value="96"></label>
</div>
<nav>{nav}</nav>
<main>
<p class="intro">Ten new styles, with no squircle tile. Each one is shown on the same 13 icons: 11 of our own designs, one file type, and one brand (Spotify). Blender rendered styles 1 to 9 on 114. Style 10 is flat SVG. The strip under each row shows the style in a dock.</p>
{"".join(sections)}
</main>
<script>
const root = document.documentElement;
const pick = (v) => {{
  root.dataset.theme = v;
  document.querySelectorAll("#theme button").forEach(b => b.classList.toggle("on", b.dataset.v === v));
  try {{ localStorage.setItem("theme", v); }} catch (e) {{}}
}};
document.querySelectorAll("#theme button").forEach(b => b.onclick = () => pick(b.dataset.v));
const size = document.getElementById("size");
size.oninput = () => {{ root.style.setProperty("--size", size.value + "px"); try {{ localStorage.setItem("size", size.value); }} catch (e) {{}} }};
try {{
  const t = localStorage.getItem("theme"); if (t) pick(t);
  const s = localStorage.getItem("size"); if (s) {{ size.value = s; size.oninput(); }}
}} catch (e) {{}}
</script>
</body>
</html>
"""

with open(os.path.join(OUT, "index.html"), "w") as f:
    f.write(page)
print("ok")
