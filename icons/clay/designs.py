import json
import math
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = sys.argv[1] if len(sys.argv) > 1 else os.path.join(HERE, "build")

BLUE = "#3584e4"
BLUE_D = "#2a63b8"
BLUE_L = "#7db4f5"
TEAL = "#2190a4"
TEAL_D = "#1a6f80"
GREEN = "#3a944a"
GREEN_L = "#57b86a"
MINT = "#7ee0a8"
YELLOW = "#c88800"
GOLD = "#f0b649"
ORANGE = "#ed5b00"
ORANGE_L = "#ff8a3d"
PEACH = "#ffb15c"
RED = "#e62d42"
PINK = "#d56199"
PINK_L = "#f08ab8"
PURPLE = "#9141ac"
VIOLET = "#7a5af5"
SLATE = "#6f8396"
SLATE_L = "#9aaabb"
SLATE_D = "#4a5868"
INK = "#2a2f40"
WHITE = "#f4f6fb"
CREAM = "#f3efe8"
PAPER = "#eef1f7"
BROWN = "#a8703a"


def rr(x0, y0, x1, y1, r):
    r = min(r, (x1 - x0) / 2, (y1 - y0) / 2)
    return (
        f"M{x0 + r:.2f} {y0:.2f}H{x1 - r:.2f}A{r} {r} 0 0 1 {x1:.2f} {y0 + r:.2f}V{y1 - r:.2f}A{r} {r} 0 0 1 {x1 - r:.2f} {y1:.2f}"
        f"H{x0 + r:.2f}A{r} {r} 0 0 1 {x0:.2f} {y1 - r:.2f}V{y0 + r:.2f}A{r} {r} 0 0 1 {x0 + r:.2f} {y0:.2f}Z"
    )


def circle(cx, cy, r, ccw=False):
    s = 0 if ccw else 1
    return f"M{cx - r:.2f} {cy:.2f}A{r} {r} 0 0 {s} {cx + r:.2f} {cy:.2f}A{r} {r} 0 0 {s} {cx - r:.2f} {cy:.2f}Z"


def ellipse(cx, cy, rx, ry):
    return f"M{cx - rx:.2f} {cy:.2f}A{rx} {ry} 0 0 1 {cx + rx:.2f} {cy:.2f}A{rx} {ry} 0 0 1 {cx - rx:.2f} {cy:.2f}Z"


def poly(points, closed=True):
    d = "M" + "L".join(f"{x:.2f} {y:.2f}" for x, y in points)
    return d + ("Z" if closed else "")


def ring(cx, cy, r0, r1):
    return circle(cx, cy, r1) + circle(cx, cy, r0, ccw=True)


def gear(cx, cy, outer, inner, teeth, hole):
    pts = []
    step = 2 * math.pi / teeth
    for i in range(teeth):
        a = i * step - math.pi / 2
        for t, r in ((-0.30, inner), (-0.17, outer), (0.17, outer), (0.30, inner)):
            pts.append((cx + r * math.cos(a + t * step), cy + r * math.sin(a + t * step)))
    d = poly(pts)
    if hole:
        d += circle(cx, cy, hole, ccw=True)
    return d


def sector(cx, cy, r0, r1, a0, a1):
    p = lambda r, a: (cx + r * math.cos(math.radians(a)), cy + r * math.sin(math.radians(a)))
    large = 1 if (a1 - a0) % 360 > 180 else 0
    x0, y0 = p(r1, a0)
    x1, y1 = p(r1, a1)
    x2, y2 = p(r0, a1)
    x3, y3 = p(r0, a0)
    if r0 <= 0:
        return f"M{cx:.2f} {cy:.2f}L{x0:.2f} {y0:.2f}A{r1} {r1} 0 {large} 1 {x1:.2f} {y1:.2f}Z"
    return f"M{x0:.2f} {y0:.2f}A{r1} {r1} 0 {large} 1 {x1:.2f} {y1:.2f}L{x2:.2f} {y2:.2f}A{r0} {r0} 0 {large} 0 {x3:.2f} {y3:.2f}Z"


def star(cx, cy, r0, r1, n=5, turn=-90):
    pts = []
    for i in range(n * 2):
        r = r1 if i % 2 == 0 else r0
        a = math.radians(turn + i * 180 / n)
        pts.append((cx + r * math.cos(a), cy + r * math.sin(a)))
    return poly(pts)


def part(pid, role, color, d, kind="fill", width=0):
    return {"id": pid, "role": role, "color": color, "d": d, "kind": kind, "width": width}


def line(pid, color, points, width=5, role="mark", closed=False):
    return part(pid, role, color, poly(points, closed), "line", width)


def text(s, color=WHITE, x=50, y=72, h=11, maxw=56, role="mark"):
    return {"s": s, "color": color, "x": x, "y": y, "h": h, "maxw": maxw, "role": role}


ICONS = {}


def icon(path, parts, texts=(), transform=None, soft=1.0, clip=None):
    shapes = [p for p in parts if "s" not in p]
    words = [p for p in parts if "s" in p] + list(texts)
    ICONS[path] = {"parts": shapes, "texts": words, "transform": transform, "soft": soft, "clip": clip}


def folder(back, front, marks=()):
    return [
        part("back", "body", back, "M12 26Q12 20 18 20H38Q41 20 43 23L46 27H82Q88 27 88 33V76Q88 82 82 82H18Q12 82 12 76Z"),
        part("front", "top", front, rr(12, 36, 88, 82, 7)),
    ] + list(marks)


def terminal(case, ink, marks=()):
    return [
        part("case", "body", case, rr(10, 16, 90, 84, 11)),
        line("chev", ink, [(27, 39), (40, 50), (27, 61)], 6),
        part("cursor", "mark", ink, rr(46, 57, 66, 63, 3)),
    ] + list(marks)


def page(color, fold, marks=(), texts=()):
    return [
        part("sheet", "body", color, "M26 10H60L80 30V84Q80 90 74 90H26Q20 90 20 84V16Q20 10 26 10Z"),
        part("fold", "top", fold, "M60 15Q60 11 63 14L76 27Q79 30 75 30H66Q60 30 60 24Z"),
    ] + list(marks)


def notepad(color, ink, marks=()):
    return [
        part("pad", "body", color, rr(20, 14, 80, 88, 9)),
        part("bind", "top", ink, "M20 26V23Q20 14 29 14H71Q80 14 80 23V26Z"),
        part("l1", "mark", SLATE_L, rr(30, 40, 70, 45, 2.5)),
        part("l2", "mark", SLATE_L, rr(30, 54, 70, 59, 2.5)),
        part("l3", "mark", SLATE_L, rr(30, 68, 56, 73, 2.5)),
    ] + list(marks)


def envelope(color, flap, marks=()):
    return [
        part("env", "body", color, rr(10, 22, 90, 78, 9)),
        line("flap", flap, [(16, 30), (50, 54), (84, 30)], 5.5),
    ] + list(marks)


def note(color, beam):
    return [
        part("head1", "body", color, "M22 72A11 8.5 -20 1 0 44 66A11 8.5 -20 1 0 22 72Z"),
        part("head2", "body", color, "M58 64A11 8.5 -20 1 0 80 58A11 8.5 -20 1 0 58 64Z"),
        part("stem1", "body", color, rr(37.5, 26, 44, 68, 3)),
        part("stem2", "body", color, rr(73.5, 18, 80, 60, 3)),
        part("beam", "top", beam, "M37.5 22L80 12V25L37.5 35Z"),
    ]


def photo(frame, sky, hills, sun):
    return [
        part("frame", "body", frame, rr(12, 18, 88, 82, 10)),
        part("sky", "top", sky, rr(19, 25, 81, 75, 5)),
        part("hills", "mark", hills, "M19 75V62L35 46L50 60L60 52L81 70V75Z"),
        part("sun", "mark", sun, circle(64, 38, 7)),
    ]


def compass(dial, bezel):
    return [
        part("dial", "body", dial, circle(50, 50, 39)),
        part("bezel", "top", bezel, ring(50, 50, 33, 39)),
        part("north", "mark", RED, poly([(50, 20), (57, 50), (43, 50)])),
        part("south", "mark", WHITE, poly([(43, 50), (57, 50), (50, 80)])),
    ]


def trash(full=False):
    p = [
        part("can", "body", SLATE, "M22 32H78L72 84Q71 88 67 88H33Q29 88 28 84Z"),
        part("lid", "top", SLATE_L, rr(16, 22, 84, 31, 4)),
        part("knob", "top", SLATE_L, rr(40, 14, 60, 24, 4)),
        line("rib1", SLATE_D, [(38, 42), (40, 78)], 4),
        line("rib2", SLATE_D, [(50, 42), (50, 78)], 4),
        line("rib3", SLATE_D, [(62, 42), (60, 78)], 4),
    ]
    if full:
        p = [
            part("can", "body", SLATE, "M22 40H78L72 86Q71 90 67 90H33Q29 90 28 86Z"),
            part("paper1", "top", CREAM, "M26 40L30 14L52 18L50 40Z"),
            part("paper2", "top", PAPER, "M46 40L58 10L76 18L68 40Z"),
            part("rim", "top", SLATE_L, rr(18, 36, 82, 44, 4)),
            line("rib1", SLATE_D, [(38, 50), (40, 80)], 4),
            line("rib2", SLATE_D, [(50, 50), (50, 80)], 4),
            line("rib3", SLATE_D, [(62, 50), (60, 80)], 4),
        ]
    return p


def bars(c1, c2, c3, pulse=WHITE):
    return [
        part("bar1", "body", c1, rr(16, 56, 32, 84, 4)),
        part("bar2", "body", c2, rr(42, 40, 58, 84, 4)),
        part("bar3", "body", c3, rr(68, 26, 84, 84, 4)),
        line("pulse", pulse, [(10, 38), (30, 38), (38, 22), (48, 54), (56, 30), (62, 38), (90, 38)], 4.5),
    ]


def archive(box, lid):
    return [
        part("box", "body", box, rr(14, 34, 86, 86, 7)),
        part("lid", "top", lid, rr(10, 20, 90, 38, 6)),
        part("zip", "mark", "#5a3d0a", rr(46, 38, 54, 74, 2)),
        part("pull", "mark", WHITE, rr(42, 62, 58, 78, 4)),
    ]


def bag(color, handle, marks=()):
    return [
        part("handle", "body", handle, ring(50, 34, 12, 18)),
        part("bag", "top", color, "M18 34H82L78 82Q77 88 71 88H29Q23 88 22 82Z"),
    ] + list(marks)


def monitor(screen, case, marks=()):
    return [
        part("stand", "body", case, poly([(40, 76), (60, 76), (64, 88), (36, 88)])),
        part("case", "body", case, rr(8, 14, 92, 76, 9)),
        part("screen", "top", screen, rr(15, 21, 85, 69, 5)),
    ] + list(marks)


def disc(color, hub):
    return [
        part("disc", "body", color, ring(50, 50, 7, 40)),
        part("label", "top", hub, ring(50, 50, 7, 17)),
    ]


def drive(color, top):
    return [
        part("case", "body", color, rr(10, 30, 90, 74, 10)),
        part("face", "top", top, rr(16, 36, 84, 56, 6)),
        part("led", "mark", MINT, circle(76, 65, 3.5)),
        part("slot", "mark", SLATE_D, rr(20, 62, 56, 67, 2.5)),
    ]


def person(color, marks=()):
    return [
        part("head", "body", color, circle(50, 34, 16)),
        part("body", "body", color, "M20 86Q20 58 50 58Q80 58 80 86Z"),
    ] + list(marks)


def calendar():
    return [
        part("page", "body", CREAM, rr(14, 18, 86, 86, 10)),
        part("head", "top", RED, "M14 38V28Q14 18 24 18H76Q86 18 86 28V38Z"),
        part("ring1", "mark", WHITE, rr(31, 11, 37, 26, 3)),
        part("ring2", "mark", WHITE, rr(63, 11, 69, 26, 3)),
    ] + [
        part(f"day{i}", "mark", "#c9c2b6" if i != 4 else RED, rr(25 + (i % 3) * 18, 48 + (i // 3) * 16, 38 + (i % 3) * 18, 59 + (i // 3) * 16, 3))
        for i in range(6)
    ]


def clockface(face, rim, hands):
    return [
        part("face", "body", face, circle(50, 50, 38)),
        part("rim", "top", rim, ring(50, 50, 32, 38)),
        line("hands", hands, [(50, 28), (50, 50), (64, 60)], 5),
        part("hub", "mark", hands, circle(50, 50, 4.5)),
    ]


def key(color, marks=()):
    return [
        part("bow", "body", color, circle(32, 50, 20) + circle(32, 50, 8, ccw=True)),
        part("shank", "body", color, rr(46, 44, 90, 56, 4)),
        part("bit1", "body", color, rr(72, 54, 80, 68, 2)),
        part("bit2", "body", color, rr(82, 54, 90, 64, 2)),
    ] + list(marks)


def magnifier(lens, handle, marks=()):
    return [
        part("handle", "body", handle, poly([(58, 64), (64, 58), (88, 82), (82, 88)])),
        part("rim", "body", handle, ring(42, 42, 22, 30)),
        part("glass", "top", lens, circle(42, 42, 22)),
    ] + list(marks)


def cloud(color):
    return part("cloud", "body", color, "M28 74Q12 74 12 60Q12 47 26 46Q28 30 46 30Q60 30 65 42Q78 40 84 52Q90 66 76 74Z")


def shield(color, marks=()):
    return [part("shield", "body", color, "M50 10L84 22V46Q84 74 50 90Q16 74 16 46V22Z")] + list(marks)


def gamepad(color, marks=()):
    return [
        part("pad", "body", color, "M30 32H70Q86 32 89 52L92 66Q94 80 82 80Q74 80 68 70H32Q26 80 18 80Q6 80 8 66L11 52Q14 32 30 32Z"),
        part("dpadh", "mark", WHITE, rr(20, 48, 38, 54, 2)),
        part("dpadv", "mark", WHITE, rr(26, 42, 32, 60, 2)),
        part("b1", "mark", PINK_L, circle(66, 46, 4.5)),
        part("b2", "mark", MINT, circle(76, 54, 4.5)),
    ] + list(marks)


def cube(top, left, right):
    return [
        part("left", "body", left, poly([(14, 30), (50, 48), (50, 90), (14, 72)])),
        part("right", "body", right, poly([(50, 48), (86, 30), (86, 72), (50, 90)])),
        part("top", "top", top, poly([(14, 30), (50, 12), (86, 30), (50, 48)])),
    ]


def speech(color, marks=()):
    return [part("bubble", "body", color, "M22 18H78Q90 18 90 30V60Q90 72 78 72H44L26 86V72H22Q10 72 10 60V30Q10 18 22 18Z")] + list(marks)


def play(color, inner=WHITE):
    return [
        part("case", "body", color, rr(10, 20, 90, 80, 12)),
        part("tri", "mark", inner, poly([(42, 36), (64, 50), (42, 64)])),
    ]


def film(color, holes):
    p = [part("strip", "body", color, rr(16, 10, 84, 90, 8))]
    for i in range(5):
        y = 16 + i * 15
        p.append(part(f"hl{i}", "mark", holes, rr(20, y, 26, y + 8, 2)))
        p.append(part(f"hr{i}", "mark", holes, rr(74, y, 80, y + 8, 2)))
    p.append(part("frame", "top", "#3a3f52", rr(32, 20, 68, 80, 4)))
    return p


def camera(color, lens):
    return [
        part("bump", "body", color, rr(32, 18, 60, 30, 4)),
        part("body", "body", color, rr(10, 26, 90, 82, 11)),
        part("lens", "top", INK, circle(50, 54, 20)),
        part("glass", "mark", lens, circle(50, 54, 12)),
        part("flash", "mark", WHITE, circle(78, 38, 4)),
    ]


def puzzle(color):
    return [part("piece", "body", color,
                 "M20 30H40Q36 16 50 16Q64 16 60 30H80V50Q94 46 94 60Q94 74 80 70V88H20V70Q34 74 34 60Q34 46 20 50Z")]


def globe(color, lines):
    return [
        part("sphere", "body", color, circle(50, 50, 38)),
        line("eq", lines, [(14, 50), (86, 50)], 4),
        part("mer", "mark", lines, ellipse(50, 50, 16, 37) + ellipse(50, 50, 11, 32), "fill"),
        line("lat1", lines, [(22, 32), (78, 32)], 4),
        line("lat2", lines, [(22, 68), (78, 68)], 4),
    ]


def speaker(color, waves):
    return [
        part("box", "body", color, poly([(14, 38), (32, 38), (54, 20), (54, 80), (32, 62), (14, 62)])),
        part("w1", "mark", waves, sector(54, 50, 12, 18, -45, 45)),
        part("w2", "mark", waves, sector(54, 50, 26, 32, -50, 50)),
    ]


def bluetooth(color):
    return [
        part("pill", "body", color, rr(24, 8, 76, 92, 26)),
        line("rune", WHITE, [(36, 34), (62, 60), (50, 72), (50, 28), (62, 40), (36, 66)], 5.5),
    ]


def house(wall, roof):
    return [
        part("wall", "body", wall, rr(22, 44, 78, 86, 5)),
        part("roof", "top", roof, poly([(10, 50), (50, 14), (90, 50), (82, 56), (50, 28), (18, 56)])),
        part("door", "mark", BROWN, rr(42, 62, 58, 86, 3)),
    ]


ICON_FILES = {}


def app(name, parts, **kw):
    icon(f"apps/{name}", parts, **kw)


def logo(slug):
    s = open(os.path.join(HERE, "brands", f"{slug}.svg")).read()
    return re.search(r'<path[^>]* d="([^"]+)"', s).group(1)


def brand(name, slug, color, marks=(), soft=0.8):
    icon(f"apps/{name}", [part("logo", "body", color, logo(slug))] + list(marks), transform="translate(10 10) scale(3.3333)", soft=soft)


app("drifter-explorer", folder(BLUE_D, BLUE, [part("star", "mark", WHITE, star(50, 59, 5, 13, 4, -90))]))
app("drifter-settings", [part("gear", "body", SLATE, gear(50, 50, 40, 31, 9, 13)), part("hub", "top", BLUE_L, ring(50, 50, 13, 20))])
app("drifter-activity", bars("#2f7a3c", GREEN, GREEN_L))
app("drifter-launchpad", [
    part("q1", "body", BLUE, rr(14, 14, 47, 47, 10)),
    part("q2", "body", PINK, rr(53, 14, 86, 47, 10)),
    part("q3", "body", GREEN_L, rr(14, 53, 47, 86, 10)),
    part("q4", "body", GOLD, rr(53, 53, 86, 86, 10)),
])
app("drifter-welcome", [
    part("drop", "body", VIOLET, "M50 10Q72 10 82 30Q92 52 80 72Q68 90 50 90Q32 90 20 72Q8 52 18 30Q28 10 50 10Z"),
    line("w1", MINT, [(26, 44), (38, 38), (50, 44), (62, 50), (74, 44)], 6),
    line("w2", PINK_L, [(26, 60), (38, 54), (50, 60), (62, 66), (74, 60)], 6),
])
app("application-x-executable", cube(BLUE_L, BLUE, BLUE_D))
app("applications-games", gamepad(VIOLET))
app("applications-other", [
    part("circ", "body", PINK, circle(32, 34, 18)),
    part("sq", "body", BLUE, rr(54, 18, 86, 50, 6)),
    part("tri", "body", GOLD, poly([(50, 56), (72, 88), (28, 88)])),
])
app("unknown", speech(SLATE, [text("?", WHITE, 50, 45, 34, 40)]))
app("help-browser", [
    part("buoy", "body", WHITE, ring(50, 50, 16, 38)),
    part("s1", "top", RED, sector(50, 50, 16, 38, -70, -20)),
    part("s2", "top", RED, sector(50, 50, 16, 38, 20, 70)),
    part("s3", "top", RED, sector(50, 50, 16, 38, 110, 160)),
    part("s4", "top", RED, sector(50, 50, 16, 38, 200, 250)),
])
app("web-browser", compass(TEAL, TEAL_D))
app("utilities-terminal", terminal(INK, MINT))
app("text-x-script", page(INK, SLATE_D, [line("chev", MINT, [(32, 46), (42, 55), (32, 64)], 5), part("cur", "mark", MINT, rr(48, 62, 66, 67, 2.5))]))
app("preferences-system-network", [
    line("l1", SLATE_L, [(50, 26), (24, 70)], 5, "body"),
    line("l2", SLATE_L, [(50, 26), (76, 70)], 5, "body"),
    line("l3", SLATE_L, [(24, 70), (76, 70)], 5, "body"),
    part("n1", "top", BLUE, circle(50, 26, 14)),
    part("n2", "top", TEAL, circle(24, 70, 14)),
    part("n3", "top", VIOLET, circle(76, 70, 14)),
])
app("printer", [
    part("paper", "body", WHITE, rr(28, 12, 72, 40, 3)),
    part("case", "top", SLATE, rr(10, 34, 90, 72, 10)),
    part("out", "mark", WHITE, rr(28, 60, 72, 88, 3)),
    part("led", "mark", MINT, circle(78, 46, 3.5)),
])
app("system-software-install", bag(BLUE, BLUE_D, [line("arrow", WHITE, [(50, 46), (50, 72)], 6), line("head", WHITE, [(40, 63), (50, 73), (60, 63)], 6)]))
app("system-software-update", [
    part("a1", "body", GREEN, sector(50, 50, 24, 36, 200, 340)),
    part("t1", "body", GREEN, poly([(86, 50), (74, 34), (98, 38)])),
    part("a2", "body", GREEN_L, sector(50, 50, 24, 36, 20, 160)),
    part("t2", "body", GREEN_L, poly([(14, 50), (26, 66), (2, 62)])),
])
app("user-trash", trash())
app("user-trash-full", trash(True))
app("multimedia-volume-control", speaker(BLUE, BLUE_L))
app("blueman", bluetooth(BLUE))
app("org.gnome.baobab", [
    part("s1", "body", BLUE, sector(50, 50, 18, 38, -90, 60)),
    part("s2", "body", GOLD, sector(50, 50, 18, 38, 64, 170)),
    part("s3", "body", PINK, sector(50, 50, 18, 38, 174, 266)),
    part("hub", "top", SLATE_L, circle(50, 50, 12)),
])
app("org.gnome.Boxes", monitor(BLUE_L, SLATE, [part("cube", "mark", BLUE_D, rr(38, 34, 62, 56, 4))]))
app("virt-manager", monitor(INK, SLATE_L, [part("w1", "mark", BLUE, rr(22, 28, 58, 50, 4)), part("w2", "mark", TEAL, rr(42, 40, 78, 62, 4))]))
app("org.gnome.Builder", [
    part("handle", "body", BROWN, poly([(30, 76), (36, 82), (74, 44), (68, 38)])),
    part("head", "top", SLATE, poly([(52, 18), (70, 12), (90, 32), (84, 50), (74, 44), (70, 36), (62, 34), (56, 26)])),
])
app("org.gnome.Calculator", [
    part("case", "body", SLATE_D, rr(20, 10, 80, 90, 10)),
    part("screen", "top", MINT, rr(28, 18, 72, 34, 4)),
] + [part(f"k{i}", "mark", ORANGE_L if i % 3 == 2 else SLATE_L, rr(28 + (i % 3) * 16, 42 + (i // 3) * 15, 40 + (i % 3) * 16, 52 + (i // 3) * 15, 3)) for i in range(9)])
app("org.gnome.Calendar", calendar())
app("org.gnome.Characters", [part("tile", "body", PEACH, circle(50, 50, 38)), text("&", WHITE, 50, 52, 44, 50)])
app("org.gnome.clocks", clockface(CREAM, SLATE, INK))
app("org.gnome.Console", terminal(INK, WHITE))
app("org.gnome.Contacts", person(TEAL, [part("badge", "top", GOLD, circle(74, 30, 10))]))
app("org.gnome.DejaDup", [
    part("vault", "body", SLATE, rr(12, 14, 88, 86, 12)),
    part("door", "top", SLATE_L, circle(50, 50, 26)),
    part("dial", "mark", INK, ring(50, 50, 8, 14)),
    line("handle", INK, [(50, 30), (50, 36)], 5),
])
app("org.gnome.DiskUtility", drive(SLATE, SLATE_L))
app("org.gnome.Epiphany", globe(BLUE, BLUE_L))
app("org.gnome.Evolution", envelope(BLUE, WHITE, [part("cal", "top", RED, rr(62, 54, 86, 78, 5)), part("cald", "mark", WHITE, rr(68, 64, 80, 72, 2))]))
app("org.gnome.FileRoller", archive("#d99a2b", GOLD))
app("org.gnome.font-viewer", [part("card", "body", PAPER, rr(12, 18, 88, 82, 10)), text("Aa", INK, 50, 51, 34, 60)])
app("org.gnome.Geary", [
    part("wing1", "body", TEAL, poly([(10, 46), (90, 14), (40, 58)])),
    part("wing2", "top", "#5fc1d1", poly([(40, 58), (90, 14), (64, 86)])),
    part("tail", "body", TEAL_D, poly([(40, 58), (46, 84), (54, 66)])),
])
app("org.gnome.gedit", notepad(PAPER, GREEN, [part("pencil", "mark", GOLD, poly([(56, 80), (84, 52), (90, 58), (62, 86), (54, 88)]))]))
app("org.gnome.TextEditor", notepad(PAPER, BLUE))
app("org.kde.kate", notepad(PAPER, VIOLET, [line("chev", VIOLET, [(62, 62), (54, 70), (62, 78)], 4.5)]))
app("org.xfce.mousepad", notepad(CREAM, TEAL))
app("org.gnome.Lollypop", [
    part("stick", "body", WHITE, rr(46, 50, 54, 92, 3)),
    part("pop", "top", PINK, circle(50, 38, 28)),
    line("swirl", PINK_L, [(50, 38), (58, 34), (58, 44), (46, 48), (38, 38), (44, 24), (60, 22), (70, 34)], 5),
])
app("org.gnome.Loupe", magnifier(PEACH, SLATE, [part("hills", "mark", ORANGE, "M24 54L34 40L44 50L50 44L60 56Q50 66 42 64Q30 62 24 54Z")]))
app("org.gnome.Maps", [
    part("map", "body", "#c9e6c4", poly([(10, 24), (36, 16), (64, 24), (90, 16), (90, 76), (64, 84), (36, 76), (10, 84)])),
    part("fold", "top", "#a9d4a2", poly([(36, 16), (64, 24), (64, 84), (36, 76)])),
    part("pin", "mark", RED, "M58 62Q44 48 44 36Q44 24 58 24Q72 24 72 36Q72 48 58 62Z" + circle(58, 36, 5, ccw=True)),
])
app("org.gnome.Nautilus", folder(BLUE_D, BLUE, [part("slot", "mark", WHITE, rr(38, 52, 62, 57, 2.5))]))
app("org.kde.dolphin", folder("#1d78c9", "#3daee9", [part("slot", "mark", WHITE, rr(38, 52, 62, 57, 2.5))]))
app("org.xfce.thunar", folder("#2a4f8a", "#4a7dc4", [part("bolt", "mark", GOLD, poly([(52, 44), (42, 60), (50, 60), (46, 74), (58, 56), (50, 56)]))]))
app("nemo", folder("#3f8a52", GREEN_L, [part("slot", "mark", WHITE, rr(38, 52, 62, 57, 2.5))]))
app("org.gnome.Papers", page(CREAM, "#d9d3c8", [part("ribbon", "top", RED, poly([(30, 10), (42, 10), (42, 40), (36, 34), (30, 40)])), part("l1", "mark", SLATE_L, rr(30, 52, 70, 57, 2.5)), part("l2", "mark", SLATE_L, rr(30, 66, 64, 71, 2.5))]))
app("org.kde.okular", page(PAPER, "#b7c4dc", [part("lens", "mark", SLATE, ring(46, 56, 10, 16)), part("h", "mark", SLATE, poly([(56, 64), (60, 60), (72, 72), (68, 76)]))]))
app("org.gnome.Rhythmbox3", disc(ORANGE_L, CREAM))
app("org.gnome.Screenshot", [
    line("c1", SLATE_L, [(14, 34), (14, 18), (30, 18)], 7, "body"),
    line("c2", SLATE_L, [(70, 18), (86, 18), (86, 34)], 7, "body"),
    line("c3", SLATE_L, [(86, 66), (86, 82), (70, 82)], 7, "body"),
    line("c4", SLATE_L, [(30, 82), (14, 82), (14, 66)], 7, "body"),
    part("dot", "top", RED, circle(50, 50, 16)),
])
app("org.gnome.seahorse.Application", key(GOLD))
app("org.gnome.Shotwell", [part("back", "body", CREAM, "M10 30L66 14L80 62L24 78Z")] + photo(WHITE, BLUE_L, BLUE, WHITE))
app("org.gnome.Showtime", play(INK, WHITE))
app("org.gnome.Snapshot", camera(SLATE_L, BLUE_L))
app("org.gnome.Software", bag(BLUE, BLUE_D, [part("tag", "mark", WHITE, star(50, 60, 6, 14))]))
app("plasmadiscover", bag("#3daee9", "#1d78c9", [part("tag", "mark", WHITE, star(50, 60, 6, 14))]))
app("org.gnome.SystemMonitor", [
    part("dial", "body", SLATE_D, sector(50, 64, 0, 40, 180, 360) + ""),
    part("a1", "top", GREEN_L, sector(50, 64, 28, 38, 180, 240)),
    part("a2", "top", GOLD, sector(50, 64, 28, 38, 242, 300)),
    part("a3", "top", RED, sector(50, 64, 28, 38, 302, 360)),
    line("needle", WHITE, [(50, 64), (68, 38)], 5),
    part("hub", "mark", WHITE, circle(50, 64, 6)),
])
app("org.gnome.Terminal", terminal(SLATE_D, MINT))
app("org.kde.konsole", terminal(INK, ORANGE_L))
app("xterm", terminal("#1e2130", WHITE, [line("x1", RED, [(66, 26), (80, 40)], 4), line("x2", RED, [(80, 26), (66, 40)], 4)]))
app("foot", terminal(PAPER, BLUE))
app("org.gnome.Weather", [
    part("sun", "body", GOLD, circle(36, 38, 22)),
    dict(cloud(WHITE), role="top"),
])
app("org.xfce.volman", [
    part("cap", "body", SLATE_L, rr(36, 10, 64, 34, 4)),
    part("stick", "top", BLUE, rr(28, 30, 72, 90, 10)),
    part("h1", "mark", INK, rr(41, 18, 47, 24, 1.5)),
    part("h2", "mark", INK, rr(53, 18, 59, 24, 1.5)),
])
app("synaptic", cube(GOLD, BROWN, "#8a5a2b"))
app("timeshift", clockface(PAPER, TEAL, INK) + [part("arrow", "top", TEAL_D, poly([(10, 34), (26, 28), (24, 44)]))])
app("gparted", drive(SLATE, SLATE_L) + [part("p1", "mark", BLUE, rr(20, 40, 44, 52, 2)), part("p2", "mark", GOLD, rr(46, 40, 62, 52, 2)), part("p3", "mark", PINK, rr(64, 40, 80, 52, 2))])
app("btop", [
    part("case", "body", INK, rr(10, 16, 90, 84, 11)),
    line("g1", MINT, [(18, 64), (30, 50), (42, 58), (54, 34), (66, 44), (82, 26)], 4.5),
    line("g2", PINK_L, [(18, 74), (32, 68), (46, 72), (60, 60), (82, 64)], 4.5),
])
app("htop", [
    part("case", "body", INK, rr(10, 16, 90, 84, 11)),
    part("b1", "mark", GREEN_L, rr(20, 28, 70, 35, 2)),
    part("b2", "mark", GREEN_L, rr(20, 42, 54, 49, 2)),
    part("b3", "mark", GOLD, rr(20, 56, 78, 63, 2)),
    part("b4", "mark", RED, rr(20, 70, 44, 77, 2)),
])
app("com.github.tchx84.Flatseal", shield(BLUE, [line("check", WHITE, [(34, 50), (46, 62), (68, 38)], 7)]))
app("io.github.flattool.Warehouse", [
    part("b1", "body", "#d99a2b", rr(10, 50, 48, 88, 5)),
    part("b2", "body", GOLD, rr(52, 50, 90, 88, 5)),
    part("b3", "top", "#e8a93a", rr(31, 12, 69, 50, 5)),
    part("t1", "mark", "#5a3d0a", rr(26, 50, 32, 62, 1.5)),
    part("t2", "mark", "#5a3d0a", rr(68, 50, 74, 62, 1.5)),
    part("t3", "mark", "#5a3d0a", rr(47, 12, 53, 24, 1.5)),
])
app("com.mattjakeman.ExtensionManager", puzzle(BLUE))
app("mandelbulber", [
    part("c1", "body", VIOLET, circle(50, 56, 30)),
    part("c2", "top", PINK, circle(50, 24, 14)),
    part("c3", "top", BLUE_L, circle(20, 56, 12)),
    part("c4", "top", BLUE_L, circle(80, 56, 12)),
    part("c5", "mark", MINT, circle(50, 86, 8)),
])
app("pinta", [
    part("handle", "body", BROWN, poly([(58, 12), (66, 12), (66, 48), (58, 48)])),
    part("ferrule", "top", SLATE_L, rr(54, 46, 70, 58, 2)),
    part("bristle", "top", RED, "M54 58H70L74 82Q62 92 50 82Z"),
    part("blob", "body", BLUE, circle(28, 76, 14)),
])
app("org.strawberrymusicplayer.strawberry", [
    part("berry", "body", RED, "M50 88Q20 74 18 48Q18 30 34 30Q42 30 50 36Q58 30 66 30Q82 30 82 48Q80 74 50 88Z"),
    part("leaf", "top", GREEN, poly([(28, 26), (42, 30), (50, 16), (58, 30), (72, 26), (62, 40), (50, 36), (38, 40)])),
] + [part(f"s{i}", "mark", GOLD, ellipse(x, y, 2.5, 3.5)) for i, (x, y) in enumerate([(36, 50), (50, 52), (64, 50), (42, 64), (58, 64), (50, 76)])])
app("heroic", shield("#3b2f8a", [part("gp", "mark", WHITE, rr(30, 40, 70, 60, 10)), part("b", "mark", PINK_L, circle(60, 50, 4))]))
app("io.github.celluloid_player.Celluloid", film(INK, SLATE_L) + [part("tri", "mark", WHITE, poly([(44, 38), (60, 50), (44, 62)]))])
app("com.usebottles.bottles", [
    part("bottle", "body", PURPLE, "M42 10H58V30Q74 38 74 56V82Q74 90 66 90H34Q26 90 26 82V56Q26 38 42 30Z"),
    part("label", "top", CREAM, rr(30, 54, 70, 74, 4)),
    part("cork", "mark", BROWN, rr(42, 6, 58, 16, 3)),
])
app("darktable", [part(f"blade{i}", "body" if i % 2 else "top", ["#5a4f8a", "#7a6faa"][i % 2], sector(50, 50, 14, 38, i * 60 - 90, i * 60 - 28)) for i in range(6)] + [part("eye", "mark", INK, circle(50, 50, 12))])
app("digikam", [
    part("body", "body", "#2b6cb0", circle(50, 50, 38)),
    part("r1", "top", "#4a90d9", ring(50, 50, 20, 28)),
    part("glass", "mark", INK, circle(50, 50, 16)),
    part("glint", "mark", WHITE, circle(44, 44, 4)),
])
app("fr.handbrake.ghb", [
    part("glass", "body", ORANGE_L, "M24 36H76L68 84Q67 90 60 90H40Q33 90 32 84Z"),
    part("rim", "top", GOLD, rr(20, 30, 80, 40, 4)),
    part("straw", "mark", RED, poly([(54, 34), (60, 34), (72, 8), (66, 6)])),
    part("slice", "mark", GOLD, sector(30, 30, 0, 14, 180, 360)),
])
app("kitty", [
    part("head", "body", SLATE_L, "M14 40L18 10L40 28H60L82 10L86 40Q92 82 50 86Q8 82 14 40Z"),
    part("e1", "mark", INK, ellipse(36, 50, 5, 7)),
    part("e2", "mark", INK, ellipse(64, 50, 5, 7)),
    part("nose", "mark", PINK, poly([(45, 62), (55, 62), (50, 68)])),
])
app("org.prismlauncher.PrismLauncher", [
    part("prism", "body", "#3d2a6e", poly([(50, 12), (88, 82), (12, 82)])),
    part("r1", "top", RED, poly([(30, 70), (70, 70), (66, 62), (34, 62)])),
    part("r2", "top", GOLD, poly([(34, 60), (66, 60), (62, 52), (38, 52)])),
    part("r3", "top", GREEN_L, poly([(39, 50), (61, 50), (57, 42), (43, 42)])),
    part("r4", "top", BLUE_L, poly([(44, 40), (56, 40), (50, 28)])),
])
app("org.shotcut.Shotcut", [
    part("board", "body", INK, rr(12, 38, 88, 86, 8)),
    part("clap", "top", "#3a3f52", poly([(12, 30), (82, 12), (86, 26), (16, 44)])),
] + [part(f"st{i}", "mark", WHITE, poly([(20 + i * 16, 28 - i * 4), (28 + i * 16, 26 - i * 4), (32 + i * 16, 39 - i * 4), (24 + i * 16, 41 - i * 4)])) for i in range(4)] + [part("dot", "mark", RED, circle(50, 62, 10))])
app("pidgin", speech(PURPLE, [part("b2", "top", "#b07cc6", "M52 46H82Q90 46 90 54V70Q90 78 82 78H80V88L70 78H52Q44 78 44 70V54Q44 46 52 46Z")]))
app("rawtherapee", [part(f"p{i}", "body", c, sector(50, 50, 10, 38, i * 60 - 90, i * 60 - 30)) for i, c in enumerate([RED, ORANGE_L, GOLD, GREEN_L, BLUE, VIOLET])] + [part("hub", "top", WHITE, circle(50, 50, 10))])
app("teams-for-linux", [
    part("p2", "body", "#7b83eb", circle(72, 34, 12)),
    part("b2", "body", "#7b83eb", "M58 82V60Q58 52 66 52H80Q88 52 88 60V76Q88 82 82 82Z"),
    part("card", "top", "#5059c9", rr(10, 26, 62, 82, 8)),
    text("T", WHITE, 36, 55, 34, 40),
])
brand("Alacritty", "alacritty", "#f46d01")
brand("bitwarden", "bitwarden", "#175ddc")
brand("blender", "blender", "#e87d0d")
brand("brave-browser", "brave", "#fb542b")
brand("codium", "vscodium", "#2f80ed")
brand("com.obsproject.Studio", "obsstudio", "#3d3a40")
brand("discord", "discord", "#5865f2")
brand("docker-desktop", "docker", "#2496ed")
brand("element-desktop", "element", "#0dbd8b")
brand("emacs", "gnuemacs", "#7f5ab6")
brand("gvim", "vim", "#019733")
def firefox(name, globe, fox, marks=(), soft=1.0):
    app(name, [
        part("globe", "body", globe, circle(12.2, 12.4, 9)),
        part("fox", "top", fox, logo("firefoxbrowser")),
    ] + list(marks), transform="translate(10 10) scale(3.3333)", soft=soft)


firefox("firefox", "#7542e5", "#ff7139", [part("belly", "mark", "#ffbd4f", "M3.4 14.5A8.73 8.73 0 0 0 20.8 14.5A10.66 10.66 0 0 1 3.4 14.5Z")])
firefox("firefox-developer-edition", "#3537b8", "#0090ed")
brand("gimp", "gimp", "#7a6f55")
brand("github-desktop", "github", "#6e40c9")
brand("inkscape", "inkscape", "#2b2f3a")
brand("joplin", "joplin", "#1071d3")
brand("krita", "krita", "#3babff")
brand("libreoffice-startcenter", "libreoffice", "#18a303")
brand("librewolf", "librewolf", "#00acff")
brand("logseq", "logseq", "#3a8f8f")
brand("lutris", "lutris", "#ff9900")
brand("mpv", "mpv", "#691f69")
brand("nvim", "neovim", "#57a143")
brand("obsidian", "obsidian", "#7c3aed")
brand("onlyoffice-desktopeditors", "onlyoffice", "#ff6f3d")
brand("org.kde.kdenlive", "kdenlive", "#527eb2")
brand("org.telegram.desktop", "telegram", "#26a5e4")
brand("org.wezfurlong.wezterm", "wezterm", "#4e49ee")
brand("postman", "postman", "#ff6c37")
brand("qbittorrent", "qbittorrent", "#2f67ba")
brand("retroarch", "retroarch", "#4b3f8f")
brand("spotify-client", "spotify", "#1ed760")
brand("steam", "steam", "#1b2838")
brand("sublime-text", "sublimetext", "#ff9800")
brand("thunderbird", "thunderbird", "#0a84ff")
brand("torbrowser", "torbrowser", "#9b5bd0")
brand("transmission", "transmission", "#d70008")
brand("virtualbox", "virtualbox", "#2f61b4")
brand("vlc", "vlcmediaplayer", "#ff8800")
def chrome_piece(turn, gap, reach=30):
    a = math.radians(turn)
    at = lambda x, y: f"{24 + (x - 24) * math.cos(a) - (y - 24) * math.sin(a):.3f} {24 + (x - 24) * math.sin(a) + (y - 24) * math.cos(a):.3f}"
    hit = lambda px, py, dx, dy, r: (lambda b, c: max((-b + s * math.sqrt(b * b - c)) for s in (-1, 1)))(dx * (px - 24) + dy * (py - 24), (px - 24) ** 2 + (py - 24) ** 2 - r * r)
    rx, ry = 24 - 6 * math.sqrt(3) + gap * math.sqrt(3) / 2, 30 - gap / 2
    dx, dy = -0.5, -math.sqrt(3) / 2
    t_in, t_out = hit(rx, ry, dx, dy, 12), hit(rx, ry, dx, dy, reach)
    return (
        f"M{at(24, 12 - gap)}L{at(24 + math.sqrt(reach * reach - (12 + gap) ** 2), 12 - gap)}"
        f"A{reach} {reach} 0 0 0 {at(rx + t_out * dx, ry + t_out * dy)}L{at(rx + t_in * dx, ry + t_in * dy)}"
        f"A12 12 0 0 1 {at(24, 12)}Z"
    )


def chrome(name, top, left, right, centre, eye=9.5, soft=1.0):
    gap = 3 * soft
    app(name, [
        part("top", "body", top, chrome_piece(0, gap)),
        part("left", "body", left, chrome_piece(240, gap)),
        part("right", "body", right, chrome_piece(120, gap)),
        part("white", "top", WHITE, circle(24, 24, 12)),
        part("centre", "mark", centre, circle(24, 24, eye)),
    ], transform="translate(10 10) scale(1.6667)", soft=soft, clip=40)


chrome("google-chrome", "#ea4335", "#34a853", "#fbbc04", "#1a73e8")
chrome("google-chrome-canary", "#f29900", "#fbbc04", "#fdd663", "#fbbc04")
chrome("chromium", "#1767d1", "#679ef5", "#afccf9", "#1a74e7", 9.75)
app("slack", [
    part("bl", "body", "#36c5f0", rr(14, 40, 48, 52, 6)), part("bs", "body", "#36c5f0", rr(36, 14, 48, 36, 6)),
    part("gl", "body", "#2eb67d", rr(48, 14, 60, 48, 6)), part("gs", "body", "#2eb67d", rr(64, 36, 86, 48, 6)),
    part("yl", "body", "#ecb22e", rr(52, 48, 86, 60, 6)), part("ys", "body", "#ecb22e", rr(52, 64, 64, 86, 6)),
    part("rl", "body", "#e01e5a", rr(40, 52, 52, 86, 6)), part("rs", "body", "#e01e5a", rr(14, 52, 36, 64, 6)),
])

app("audacity", [
    part("band", "body", SLATE_D, sector(50, 54, 30, 38, 180, 360)),
    part("cupl", "top", "#ff6a00", rr(10, 50, 30, 84, 9)),
    part("cupr", "top", "#ff6a00", rr(70, 50, 90, 84, 9)),
    part("wave", "mark", "#2f6fe0", poly([(36, 66), (40, 54), (44, 74), (48, 46), (52, 80), (56, 52), (60, 70), (64, 62)], closed=False), "line", 4),
])
app("code", [
    part("left", "body", "#0065a9", poly([(12, 38), (20, 32), (72, 74), (72, 90), (64, 88), (12, 62)])),
    part("lower", "body", "#007acc", poly([(12, 62), (20, 68), (72, 26), (72, 10), (64, 12), (12, 38)])),
    part("right", "top", "#1f9cf0", poly([(64, 12), (72, 10), (90, 20), (90, 80), (72, 90), (64, 88)])),
])
app("dbeaver", [
    part("db", "body", BROWN, "M18 24Q18 12 50 12Q82 12 82 24V76Q82 88 50 88Q18 88 18 76Z"),
    part("lid", "top", "#c99363", ellipse(50, 24, 32, 12)),
    line("r1", "#7a5a44", [(18, 46), (34, 52), (50, 54), (66, 52), (82, 46)], 4),
    line("r2", "#7a5a44", [(18, 64), (34, 70), (50, 72), (66, 70), (82, 64)], 4),
])
app("jetbrains-toolbox", [
    part("t1", "body", "#ff8a3d", poly([(10, 12), (62, 20), (30, 62)])),
    part("t2", "body", "#ff318c", poly([(40, 10), (90, 30), (70, 70)])),
    part("t3", "body", VIOLET, poly([(14, 52), (74, 50), (54, 90)])),
    part("sq", "top", "#1e2130", rr(28, 28, 72, 72, 4)),
    part("bar", "mark", WHITE, rr(34, 60, 52, 65, 1.5)),
    text("JB", WHITE, 46, 43, 16, 30),
])
app("keepassxc", [
    part("shackle", "body", SLATE_L, sector(50, 42, 14, 24, 180, 360)),
    part("legl", "body", SLATE_L, rr(26, 40, 36, 52, 2)),
    part("legr", "body", SLATE_L, rr(64, 40, 74, 52, 2)),
    part("lock", "top", "#6cac4d", rr(16, 46, 84, 90, 10)),
    part("hole", "mark", WHITE, circle(50, 62, 7)),
    part("slit", "mark", WHITE, rr(47, 64, 53, 78, 2)),
])
app("signal-desktop", speech("#3b45fd", [part("in", "top", WHITE, ring(50, 45, 14, 20))]))
app("zed", [part("z", "body", "#084ccf", poly([(14, 12), (86, 12), (86, 26), (36, 74), (86, 74), (86, 88), (14, 88), (14, 74), (64, 26), (14, 26)]))])
app("Zoom", [
    part("disc", "body", "#0b5cff", circle(50, 50, 40)),
    part("cam", "top", WHITE, rr(22, 36, 62, 64, 7)),
    part("lens", "top", WHITE, poly([(64, 46), (78, 36), (78, 64), (64, 54)])),
])


def lo(name, corner, kind, label=None):
    marks = sym(kind, corner)
    p = [
        part("sheet", "body", WHITE, "M26 10H60L80 30V84Q80 90 74 90H26Q20 90 20 84V16Q20 10 26 10Z"),
        part("fold", "top", corner, "M60 15Q60 11 63 14L76 27Q79 30 75 30H66Q60 30 60 24Z"),
    ] + [dict(m, color={WHITE: corner, corner: WHITE}.get(m["color"], m["color"])) for m in marks]
    icon(f"apps/{name}", p, texts=[text(label, corner, 50, 50, 26, 40)] if label else ())


FOLDER_MARKS = {
    "folder": [part("slot", "mark", WHITE, rr(38, 52, 62, 57, 2.5))],
    "folder-documents": [part("doc", "mark", WHITE, "M40 46H54L60 52V72H40Z"), part("l", "mark", BLUE, rr(44, 58, 56, 61, 1.5))],
    "folder-download": [line("a", WHITE, [(50, 46), (50, 70)], 5.5), line("h", WHITE, [(41, 61), (50, 70), (59, 61)], 5.5)],
    "folder-music": [part("n1", "mark", WHITE, circle(44, 68, 5)), part("n2", "mark", WHITE, rr(46, 46, 50, 68, 1.5)), part("n3", "mark", WHITE, poly([(46, 46), (60, 50), (60, 56), (50, 53)]))],
    "folder-pictures": [part("fr", "mark", WHITE, "M36 48H64V72H36Z" + "M40 68L48 58L54 64L57 61L60 68Z")],
    "folder-remote": [part("cl", "mark", WHITE, "M40 70Q32 70 32 63Q32 57 39 56Q40 48 50 48Q57 48 60 54Q68 54 68 62Q68 70 60 70Z")],
    "folder-trash": [part("can", "mark", WHITE, "M41 52H59L57 72H43Z"), part("lid", "mark", WHITE, rr(38, 46, 62, 50, 2))],
    "folder-videos": [part("tri", "mark", WHITE, poly([(44, 48), (60, 59), (44, 70)]))],
}
for name, marks in FOLDER_MARKS.items():
    icon(f"places/{name}", folder(BLUE_D, BLUE, marks))
icon("places/user-desktop", monitor(BLUE_L, SLATE, [part("dock", "mark", WHITE, rr(30, 60, 70, 65, 2.5))]))
icon("places/user-home", house(PEACH, RED))
icon("devices/drive-harddisk", drive(SLATE, SLATE_L))
icon("devices/drive-removable-media-usb", ICONS["apps/org.xfce.volman"]["parts"])

FAMILY = {
    "doc": BLUE, "sheet": GREEN, "slides": ORANGE_L, "draw": GOLD, "pdf": RED, "archive": "#d99a2b",
    "image": TEAL, "audio": PINK, "video": PURPLE, "font": SLATE, "code": INK, "text": SLATE, "disk": SLATE,
    "pkg": BROWN, "cal": RED, "card": TEAL, "key": GOLD, "web": BLUE, "data": VIOLET,
}


def sym(kind, c):
    if kind == "lines":
        return [part("s1", "mark", WHITE, rr(30, 38, 70, 43, 2.5)), part("s2", "mark", WHITE, rr(30, 48, 70, 53, 2.5)), part("s3", "mark", WHITE, rr(30, 58, 56, 63, 2.5))]
    if kind == "grid":
        return [part(f"g{i}", "mark", WHITE, rr(30 + (i % 3) * 14, 36 + (i // 3) * 10, 40 + (i % 3) * 14, 43 + (i // 3) * 10, 1.5)) for i in range(6)] + [part("g6", "mark", WHITE, rr(30, 56, 40, 63, 1.5)), part("g7", "mark", WHITE, rr(44, 56, 54, 63, 1.5)), part("g8", "mark", WHITE, rr(58, 56, 68, 63, 1.5))]
    if kind == "chart":
        return [part("b1", "mark", WHITE, rr(32, 50, 40, 64, 2)), part("b2", "mark", WHITE, rr(46, 40, 54, 64, 2)), part("b3", "mark", WHITE, rr(60, 34, 68, 64, 2))]
    if kind == "shapes":
        return [part("c", "mark", WHITE, circle(40, 46, 9)), part("t", "mark", WHITE, poly([(56, 38), (68, 60), (44, 60)]))]
    if kind == "zip":
        return [part(f"z{i}", "mark", "#5a3d0a", rr(46 if i % 2 else 50, 30 + i * 6, 50 if i % 2 else 54, 34 + i * 6, 1)) for i in range(5)] + [part("pull", "mark", WHITE, rr(44, 58, 56, 66, 3))]
    if kind == "image":
        return [part("hills", "mark", WHITE, "M28 64L40 46L50 56L56 50L72 64Z"), part("sun", "mark", WHITE, circle(62, 38, 5))]
    if kind == "note":
        return [part("h", "mark", WHITE, ellipse(43, 60, 7, 5.5)), part("st", "mark", WHITE, rr(47, 32, 52, 60, 2)), part("fl", "mark", WHITE, poly([(47, 32), (64, 38), (64, 46), (52, 42)]))]
    if kind == "play":
        return [part("tri", "mark", WHITE, poly([(40, 34), (64, 49), (40, 64)]))]
    if kind == "code":
        return [line("l", WHITE, [(40, 38), (30, 49), (40, 60)], 5), line("r", WHITE, [(60, 38), (70, 49), (60, 60)], 5), line("s", WHITE, [(54, 34), (46, 64)], 4)]
    if kind == "disc":
        return [part("d", "mark", WHITE, ring(50, 48, 5, 17))]
    if kind == "box":
        return [part("bx", "mark", WHITE, poly([(34, 38), (50, 30), (66, 38), (66, 58), (50, 66), (34, 58)])), line("e", c, [(34, 38), (50, 46), (66, 38)], 3), line("e2", c, [(50, 46), (50, 66)], 3)]
    if kind == "gear":
        return [part("gr", "mark", WHITE, gear(50, 48, 17, 13, 8, 6))]
    if kind == "cal":
        return [part("top", "mark", WHITE, rr(32, 34, 68, 42, 2))] + [part(f"d{i}", "mark", WHITE, rr(32 + (i % 3) * 13, 46 + (i // 3) * 9, 42 + (i % 3) * 13, 52 + (i // 3) * 9, 1.5)) for i in range(6)]
    if kind == "person":
        return [part("hd", "mark", WHITE, circle(50, 40, 8)), part("bd", "mark", WHITE, "M36 64Q36 50 50 50Q64 50 64 64Z")]
    if kind == "key":
        return [part("bw", "mark", WHITE, ring(40, 48, 4, 10)), part("sh", "mark", WHITE, rr(48, 45, 70, 51, 2)), part("bt", "mark", WHITE, rr(62, 49, 67, 58, 1.5))]
    if kind == "link":
        return [part("k1", "mark", WHITE, ring(42, 52, 5, 10)), part("k2", "mark", WHITE, ring(58, 44, 5, 10))]
    if kind == "db":
        return [part("db", "mark", WHITE, "M34 36Q34 30 50 30Q66 30 66 36V62Q66 68 50 68Q34 68 34 62Z"), line("r1", c, [(34, 46), (50, 50), (66, 46)], 3), line("r2", c, [(34, 56), (50, 60), (66, 56)], 3)]
    if kind == "rocket":
        return [part("rk", "mark", WHITE, "M50 28Q62 38 60 56L56 62H44L40 56Q38 38 50 28Z"), part("fin", "mark", WHITE, poly([(40, 54), (32, 64), (42, 62)])), part("fin2", "mark", WHITE, poly([(60, 54), (68, 64), (58, 62)]))]
    if kind == "ticket":
        return [part("tk", "mark", WHITE, "M30 36H70V44Q66 48 70 52V60H30V52Q34 48 30 44Z"), line("ar", c, [(50, 40), (50, 54)], 3), line("ah", c, [(45, 50), (50, 55), (55, 50)], 3)]
    if kind == "bits":
        return [part(f"b{i}", "mark", WHITE, rr(30 + (i % 4) * 11, 34 + (i // 4) * 14, 37 + (i % 4) * 11, 44 + (i // 4) * 14, 2) if (i * 7) % 3 else ring(33.5 + (i % 4) * 11, 39 + (i // 4) * 14, 1.6, 4.4)) for i in range(8)]
    if kind == "aa":
        return []
    return []


def ftype(name, family, kind, label, color=None):
    c = color or FAMILY[family]
    fold = "#ffffff"
    texts = [text(label, WHITE, 50, 76, 10, 52)]
    if kind == "aa":
        texts.append(text("Aa", WHITE, 50, 46, 24, 44))
    icon(f"mimetypes/{name}", page(c, fold, sym(kind, c)), texts=texts)


MIME = [
    ("application-epub+zip", "doc", "lines", "EPUB", GREEN), ("application-gzip", "archive", "zip", "GZ"),
    ("application-illustrator", "draw", "shapes", "AI", ORANGE), ("application-json", "code", "code", "JSON", "#c88800"),
    ("application-msword", "doc", "lines", "DOC"), ("application-octet-stream", "data", "bits", "BIN"),
    ("application-pdf", "pdf", "lines", "PDF"), ("application-pkcs8", "key", "key", "KEY"),
    ("application-rtf", "doc", "lines", "RTF"), ("application-sql", "code", "db", "SQL", "#e38c00"),
    ("application-toml", "code", "gear", "TOML", "#9c4221"), ("application-vnd.android.package-archive", "pkg", "box", "APK", GREEN),
    ("application-vnd.appimage", "pkg", "box", "APPIMAGE", BLUE), ("application-vnd.debian.binary-package", "pkg", "box", "DEB", RED),
    ("application-vnd.flatpak.ref", "pkg", "box", "FLATPAK", "#4a90d9"), ("application-vnd.ms-excel", "sheet", "grid", "XLS"),
    ("application-vnd.oasis.opendocument.graphics", "draw", "shapes", "ODG"), ("application-vnd.oasis.opendocument.presentation", "slides", "chart", "ODP"),
    ("application-vnd.oasis.opendocument.spreadsheet", "sheet", "grid", "ODS"), ("application-vnd.oasis.opendocument.text", "doc", "lines", "ODT"),
    ("application-vnd.openxmlformats-officedocument.presentationml.presentation", "slides", "chart", "PPTX"),
    ("application-vnd.openxmlformats-officedocument.spreadsheetml.sheet", "sheet", "grid", "XLSX"),
    ("application-vnd.openxmlformats-officedocument.wordprocessingml.document", "doc", "lines", "DOCX"),
    ("application-vnd.rar", "archive", "zip", "RAR"), ("application-vnd.snap", "pkg", "box", "SNAP", ORANGE),
    ("application-vnd.sqlite3", "data", "db", "DB"), ("application-x-7z-compressed", "archive", "zip", "7Z"),
    ("application-x-bittorrent", "web", "ticket", "TORRENT", "#2f67ba"), ("application-x-blender", "draw", "disc", "BLEND", "#e87d0d"),
    ("application-x-bzip", "archive", "zip", "BZ2"), ("application-x-cd-image", "disk", "disc", "ISO"),
    ("application-x-desktop", "pkg", "rocket", "DESKTOP", VIOLET), ("application-x-java-archive", "archive", "zip", "JAR", "#e76f00"),
    ("application-x-krita", "draw", "image", "KRA", "#3babff"), ("application-x-ms-dos-executable", "pkg", "gear", "EXE", BLUE_D),
    ("application-x-msi", "pkg", "gear", "MSI", BLUE_D), ("application-x-mswinurl", "web", "link", "URL"),
    ("application-x-php", "code", "code", "PHP", "#777bb4"), ("application-x-raw-disk-image", "disk", "disc", "IMG"),
    ("application-x-rpm", "pkg", "box", "RPM", "#cc0000"), ("application-x-ruby", "code", "code", "RB", "#cc342d"),
    ("application-x-sharedlib", "data", "gear", "SO"), ("application-x-shellscript", "code", "code", "SH"),
    ("application-x-tar", "archive", "zip", "TAR"), ("application-x-xz", "archive", "zip", "XZ"),
    ("application-x-yaml", "code", "gear", "YAML", "#cb171e"), ("application-xml", "code", "code", "XML", ORANGE_L),
    ("application-zip", "archive", "zip", "ZIP"), ("application-zstd", "archive", "zip", "ZST"),
    ("audio-aac", "audio", "note", "AAC"), ("audio-flac", "audio", "note", "FLAC"), ("audio-midi", "audio", "note", "MIDI"),
    ("audio-mp4", "audio", "note", "M4A"), ("audio-mpeg", "audio", "note", "MP3"), ("audio-ogg", "audio", "note", "OGG"),
    ("audio-x-opus+ogg", "audio", "note", "OPUS"), ("audio-x-wav", "audio", "note", "WAV"),
    ("font-otf", "font", "aa", "OTF"), ("font-ttf", "font", "aa", "TTF"),
    ("image-avif", "image", "image", "AVIF"), ("image-bmp", "image", "image", "BMP"), ("image-gif", "image", "image", "GIF"),
    ("image-heic", "image", "image", "HEIC"), ("image-jpeg", "image", "image", "JPG"), ("image-png", "image", "image", "PNG"),
    ("image-svg+xml", "image", "shapes", "SVG", ORANGE), ("image-tiff", "image", "image", "TIFF"),
    ("image-vnd.adobe.photoshop", "image", "image", "PSD", "#31a8ff"), ("image-vnd.microsoft.icon", "image", "image", "ICO"),
    ("image-webp", "image", "image", "WEBP"), ("image-x-dcraw", "image", "image", "RAW"), ("image-x-xcf", "image", "image", "XCF", "#7a6f55"),
    ("text-calendar", "cal", "cal", "ICS"), ("text-css", "code", "code", "CSS", "#1572b6"), ("text-csv", "sheet", "grid", "CSV"),
    ("text-html", "code", "code", "HTML", "#e34f26"), ("text-javascript", "code", "code", "JS", "#c9a800"),
    ("text-markdown", "text", "lines", "MD"), ("text-plain", "text", "lines", "TXT"), ("text-vcard", "card", "person", "VCF"),
    ("text-x-c++src", "code", "code", "C++", "#00599c"), ("text-x-chdr", "code", "code", "H", "#5c6bc0"),
    ("text-x-conf", "text", "gear", "CONF"), ("text-x-csrc", "code", "code", "C", "#5c6bc0"),
    ("text-x-dotenv", "code", "key", "ENV", "#b8a300"), ("text-x-go", "code", "code", "GO", "#00add8"),
    ("text-x-ini", "text", "gear", "INI"), ("text-x-java", "code", "code", "JAVA", "#e76f00"),
    ("text-x-log", "text", "lines", "LOG"), ("text-x-lua", "code", "code", "LUA", "#2c2d72"),
    ("text-x-python", "code", "code", "PY", "#3776ab"), ("text-x-rust", "code", "code", "RS", "#ce422b"),
    ("text-x-tex", "text", "lines", "TEX", TEAL), ("text-x-typescript", "code", "code", "TS", "#3178c6"),
    ("video-mp4", "video", "play", "MP4"), ("video-quicktime", "video", "play", "MOV"), ("video-webm", "video", "play", "WEBM"),
    ("video-x-flv", "video", "play", "FLV"), ("video-x-m4v", "video", "play", "M4V"), ("video-x-matroska", "video", "play", "MKV"),
    ("video-x-ms-wmv", "video", "play", "WMV"), ("video-x-msvideo", "video", "play", "AVI"),
]
for row in MIME:
    ftype(*row)
lo("libreoffice-writer", "#083fa6", "lines")
lo("libreoffice-calc", "#007c3c", "grid")
lo("libreoffice-impress", "#d0120d", "chart")
lo("libreoffice-draw", "#cb6d30", "shapes")
lo("libreoffice-base", "#7324a9", "db")
lo("libreoffice-math", "#c10018", "none", "fx")


def write():
    os.makedirs(OUT, exist_ok=True)
    meta = {}
    for path, s in ICONS.items():
        key = path.replace("/", "__")
        body = []
        for p in s["parts"]:
            if p["kind"] == "line":
                attrs = f'fill="none" stroke="{p["color"]}" stroke-width="{p["width"]}" stroke-linecap="round" stroke-linejoin="round"'
            else:
                attrs = f'fill="{p["color"]}" fill-rule="evenodd"'
            body.append(f'<path id="{p["id"]}" {attrs} d="{p["d"]}"/>')
        inner = "".join(body)
        if s["transform"]:
            inner = f'<g transform="{s["transform"]}">{inner}</g>'
        ref = '<path id="zzref" fill="#000" d="M0 0H100V100H0Z"/>'
        with open(os.path.join(OUT, f"{key}.svg"), "w") as f:
            f.write(f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">{inner}</svg>')
        with open(os.path.join(OUT, f"{key}.blend.svg"), "w") as f:
            f.write(f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">{ref}{inner}</svg>')
        meta[key] = {"path": path, "parts": s["parts"], "texts": s["texts"], "soft": s["soft"], "clip": s["clip"]}
    with open(os.path.join(OUT, "subjects.json"), "w") as f:
        json.dump(meta, f, indent=1)
    return meta


if __name__ == "__main__":
    print(len(write()))
