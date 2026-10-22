import json
import math
import os
import sys

OUT = sys.argv[1]

BLUE = "#3584e4"
TEAL = "#2190a4"
GREEN = "#3a944a"
YELLOW = "#c88800"
ORANGE = "#ed5b00"
RED = "#e62d42"
PINK = "#d56199"
PURPLE = "#9141ac"
SLATE = "#6f8396"
WHITE = "#f4f6fb"
INK = "#1e2130"


def rr(x0, y0, x1, y1, r):
    return (
        f"M{x0 + r} {y0}H{x1 - r}A{r} {r} 0 0 1 {x1} {y0 + r}V{y1 - r}A{r} {r} 0 0 1 {x1 - r} {y1}"
        f"H{x0 + r}A{r} {r} 0 0 1 {x0} {y1 - r}V{y0 + r}A{r} {r} 0 0 1 {x0 + r} {y0}Z"
    )


def circle(cx, cy, r, ccw=False):
    s = 0 if ccw else 1
    return f"M{cx - r} {cy}A{r} {r} 0 0 {s} {cx + r} {cy}A{r} {r} 0 0 {s} {cx - r} {cy}Z"


def poly(points, closed=True):
    d = "M" + "L".join(f"{x:.2f} {y:.2f}" for x, y in points)
    return d + ("Z" if closed else "")


def gear(cx, cy, outer, inner, teeth, hole):
    pts = []
    step = 2 * math.pi / teeth
    for i in range(teeth):
        a = i * step - math.pi / 2
        for t, r in ((-0.30, inner), (-0.17, outer), (0.17, outer), (0.30, inner)):
            pts.append((cx + r * math.cos(a + t * step), cy + r * math.sin(a + t * step)))
    return poly(pts) + circle(cx, cy, hole, ccw=True)


def ring(cx, cy, r0, r1):
    return circle(cx, cy, r1) + circle(cx, cy, r0, ccw=True)


SUBJECTS = {}


def subject(name, label, hue, parts):
    SUBJECTS[name] = {"label": label, "hue": hue, "parts": parts}


def part(pid, role, color, d, kind="fill", width=0):
    return {"id": pid, "role": role, "color": color, "d": d, "kind": kind, "width": width}


subject("files", "Files", BLUE, [
    part("back", "body", "#2a63b8", "M12 26Q12 20 18 20H38Q41 20 43 23L46 27H82Q88 27 88 33V76Q88 82 82 82H18Q12 82 12 76Z"),
    part("front", "top", BLUE, rr(12, 36, 88, 82, 7)),
    part("slot", "mark", WHITE, rr(38, 52, 62, 57, 2.5)),
])

subject("terminal", "Terminal", SLATE, [
    part("case", "body", "#2a2f40", rr(10, 16, 90, 84, 11)),
    part("chev", "mark", "#7ee0a8", poly([(27, 39), (40, 50), (27, 61)], closed=False), "line", 6),
    part("cursor", "mark", "#7ee0a8", rr(46, 57, 66, 63, 3)),
])

subject("browser", "Browser", TEAL, [
    part("dial", "body", TEAL, circle(50, 50, 39)),
    part("bezel", "top", "#1a6f80", ring(50, 50, 33, 39)),
    part("north", "mark", RED, poly([(50, 20), (57, 50), (43, 50)])),
    part("south", "mark", WHITE, poly([(43, 50), (57, 50), (50, 80)])),
])

subject("settings", "Settings", SLATE, [
    part("gear", "body", SLATE, gear(50, 50, 40, 31, 9, 13)),
    part("hub", "top", "#9aaabb", ring(50, 50, 13, 20)),
])

subject("mail", "Mail", BLUE, [
    part("env", "body", BLUE, rr(10, 22, 90, 78, 9)),
    part("flap", "mark", WHITE, poly([(16, 30), (50, 54), (84, 30)], closed=False), "line", 5.5),
])

subject("music", "Music", PINK, [
    part("head1", "body", PINK, "M22 72A11 8.5 -20 1 0 44 66A11 8.5 -20 1 0 22 72Z"),
    part("head2", "body", PINK, "M58 64A11 8.5 -20 1 0 80 58A11 8.5 -20 1 0 58 64Z"),
    part("stem1", "body", PINK, rr(37.5, 26, 44, 68, 3)),
    part("stem2", "body", PINK, rr(73.5, 18, 80, 60, 3)),
    part("beam", "top", "#f08ab8", "M37.5 22L80 12V25L37.5 35Z"),
])

subject("photos", "Photos", ORANGE, [
    part("frame", "body", "#f3efe8", rr(12, 18, 88, 82, 10)),
    part("sky", "top", "#ffb15c", rr(19, 25, 81, 75, 5)),
    part("hills", "mark", ORANGE, "M19 75V62L35 46L50 60L60 52L81 70V75Z"),
    part("sun", "mark", "#fff0c8", circle(64, 38, 7)),
])

subject("calendar", "Calendar", RED, [
    part("page", "body", "#f3efe8", rr(14, 18, 86, 86, 10)),
    part("head", "top", RED, "M14 38V28Q14 18 24 18H76Q86 18 86 28V38Z"),
    part("ring1", "mark", WHITE, rr(31, 11, 37, 26, 3)),
    part("ring2", "mark", WHITE, rr(63, 11, 69, 26, 3)),
] + [
    part(f"day{i}", "mark", "#c9c2b6" if i != 4 else RED, rr(25 + (i % 3) * 18, 48 + (i // 3) * 16, 25 + (i % 3) * 18 + 13, 48 + (i // 3) * 16 + 11, 3))
    for i in range(6)
])

subject("trash", "Trash", SLATE, [
    part("can", "body", SLATE, "M22 32H78L72 84Q71 88 67 88H33Q29 88 28 84Z"),
    part("lid", "top", "#9aaabb", rr(16, 22, 84, 31, 4)),
    part("knob", "top", "#9aaabb", rr(40, 14, 60, 24, 4)),
    part("rib1", "mark", "#4a5868", poly([(38, 42), (40, 78)], closed=False), "line", 4),
    part("rib2", "mark", "#4a5868", poly([(50, 42), (50, 78)], closed=False), "line", 4),
    part("rib3", "mark", "#4a5868", poly([(62, 42), (60, 78)], closed=False), "line", 4),
])

subject("activity", "Activity", GREEN, [
    part("bar1", "body", "#2f7a3c", rr(16, 56, 32, 84, 4)),
    part("bar2", "body", GREEN, rr(42, 40, 58, 84, 4)),
    part("bar3", "body", "#57b86a", rr(68, 26, 84, 84, 4)),
    part("pulse", "mark", WHITE, poly([(10, 38), (30, 38), (38, 22), (48, 54), (56, 30), (62, 38), (90, 38)], closed=False), "line", 4.5),
])

subject("document", "Document", BLUE, [
    part("sheet", "body", "#eef1f7", "M26 10H60L80 30V84Q80 90 74 90H26Q20 90 20 84V16Q20 10 26 10Z"),
    part("fold", "top", "#b7c4dc", "M60 15Q60 11 63 14L76 27Q79 30 75 30H66Q60 30 60 24Z"),
    part("ln1", "mark", BLUE, rr(30, 44, 70, 49, 2.5)),
    part("ln2", "mark", BLUE, rr(30, 56, 70, 61, 2.5)),
    part("ln3", "mark", BLUE, rr(30, 68, 56, 73, 2.5)),
])

subject("archive", "Archive", YELLOW, [
    part("box", "body", "#d99a2b", rr(14, 34, 86, 86, 7)),
    part("lid", "top", "#f0b649", rr(10, 20, 90, 38, 6)),
    part("zip", "mark", "#5a3d0a", rr(46, 38, 54, 74, 2)),
    part("pull", "mark", "#f4f6fb", rr(42, 62, 58, 78, 4)),
])

subject("apps", "Apps", PURPLE, [
    part("q1", "body", BLUE, rr(14, 14, 47, 47, 10)),
    part("q2", "body", PINK, rr(53, 14, 86, 47, 10)),
    part("q3", "body", "#57b86a", rr(14, 53, 47, 86, 10)),
    part("q4", "body", "#f0b649", rr(53, 53, 86, 86, 10)),
])

SPOTIFY = (
    "M12 0C5.4 0 0 5.4 0 12s5.4 12 12 12 12-5.4 12-12S18.66 0 12 0zm5.521 17.34c-.24.359-.66.48-1.021.24-2.82-1.74-6.36-2.101-10.561-1.141"
    "-.418.122-.779-.179-.899-.539-.12-.421.18-.78.54-.9 4.56-1.021 8.52-.6 11.64 1.32.42.18.479.659.301 1.02zm1.44-3.3c-.301.42-.841.6"
    "-1.262.3-3.239-1.98-8.159-2.58-11.939-1.38-.479.12-1.02-.12-1.14-.6-.12-.48.12-1.021.6-1.141C9.6 9.9 15 10.561 18.72 12.84c.361.181"
    ".54.78.241 1.2zm.12-3.36C15.24 8.4 8.82 8.16 5.16 9.301c-.6.179-1.2-.181-1.38-.721-.18-.601.18-1.2.72-1.381 4.26-1.26 11.28-1.02"
    " 15.721 1.621.539.3.719 1.02.419 1.56-.299.421-1.02.599-1.559.3z"
)
subject("spotify", "Spotify", "#1ed760", [
    part("logo", "body", "#1ed760", SPOTIFY),
])
SUBJECTS["spotify"]["transform"] = "translate(12 12) scale(3.1667)"

os.makedirs(OUT, exist_ok=True)
for name, s in SUBJECTS.items():
    t = s.get("transform")
    body = []
    for p in s["parts"]:
        if p["kind"] == "line":
            attrs = f'fill="none" stroke="{p["color"]}" stroke-width="{p["width"]}" stroke-linecap="round" stroke-linejoin="round"'
        else:
            attrs = f'fill="{p["color"]}"'
        body.append(f'<path id="{p["id"]}" {attrs} d="{p["d"]}"/>')
    inner = "".join(body)
    if t:
        inner = f'<g transform="{t}">{inner}</g>'
    ref = '<path id="zzref" fill="#000" d="M0 0H100V100H0Z"/>'
    with open(os.path.join(OUT, f"{name}.svg"), "w") as f:
        f.write(f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">{inner}</svg>')
    with open(os.path.join(OUT, f"{name}.blend.svg"), "w") as f:
        f.write(f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">{ref}{inner}</svg>')

with open(os.path.join(OUT, "subjects.json"), "w") as f:
    json.dump(SUBJECTS, f, indent=1)
print(len(SUBJECTS))
