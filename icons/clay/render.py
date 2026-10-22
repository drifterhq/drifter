import bpy
import json
import os
import sys
from mathutils import Vector

argv = sys.argv[sys.argv.index("--") + 1:]
SRC = argv[0]
OUT = argv[1]
ONLY = set(argv[2].split(",")) if len(argv) > 2 and argv[2] else None
SIZE = int(os.environ.get("ICON_SIZE", "512"))
SAMPLES = int(os.environ.get("ICON_SAMPLES", "96"))
FONTS = [
    "/usr/share/fonts/redhat/RedHatText-Bold.otf",
    "/usr/share/fonts/google-droid-sans-fonts/DroidSans-Bold.ttf",
    "/usr/share/fonts/liberation-sans/LiberationSans-Bold.ttf",
]
SUBJECTS = json.load(open(os.path.join(SRC, "subjects.json")))
WHITE = (1.0, 1.0, 1.0)


def lin(h):
    h = h.lstrip("#")
    c = [int(h[i:i + 2], 16) / 255 for i in (0, 2, 4)]
    return tuple(x / 12.92 if x <= 0.04045 else ((x + 0.055) / 1.055) ** 2.4 for x in c)


def mix(a, b, t):
    return tuple(x * (1 - t) + y * t for x, y in zip(a, b))


def clay(color):
    m = bpy.data.materials.new("clay")
    m.use_nodes = True
    b = next(n for n in m.node_tree.nodes if n.type == "BSDF_PRINCIPLED")
    b.inputs["Base Color"].default_value = (*mix(lin(color), WHITE, 0.08), 1)
    b.inputs["Roughness"].default_value = 0.62
    b.inputs["Subsurface Weight"].default_value = 0.12
    b.inputs["Subsurface Radius"].default_value = (0.05, 0.03, 0.02)
    b.inputs["Specular IOR Level"].default_value = 0.3
    return m


def normal_material():
    m = bpy.data.materials.new("normal")
    m.use_nodes = True
    nt = m.node_tree
    for n in list(nt.nodes):
        nt.nodes.remove(n)
    out = nt.nodes.new("ShaderNodeOutputMaterial")
    geo = nt.nodes.new("ShaderNodeNewGeometry")
    half = nt.nodes.new("ShaderNodeVectorMath")
    half.operation = "MULTIPLY_ADD"
    half.inputs[1].default_value = (0.5, 0.5, 0.5)
    half.inputs[2].default_value = (0.5, 0.5, 0.5)
    e = nt.nodes.new("ShaderNodeEmission")
    nt.links.new(geo.outputs["Normal"], half.inputs[0])
    nt.links.new(half.outputs[0], e.inputs["Color"])
    nt.links.new(e.outputs[0], out.inputs[0])
    return m


def load(key):
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_curve.svg(filepath=os.path.join(SRC, f"{key}.blend.svg"))
    ref = bpy.data.objects["zzref"]
    pts = [p.co for s in ref.data.splines for p in (s.bezier_points if s.type == "BEZIER" else s.points)]
    x0, x1 = min(p.x for p in pts), max(p.x for p in pts)
    y0, y1 = min(p.y for p in pts), max(p.y for p in pts)
    k = 1.0 / (x1 - x0)
    cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
    bpy.data.objects.remove(ref)
    meta = {p["id"]: p for p in SUBJECTS[key]["parts"]}
    parts = []
    for o in list(bpy.data.objects):
        for s in o.data.splines:
            for p in s.bezier_points:
                for attr in ("co", "handle_left", "handle_right"):
                    v = getattr(p, attr)
                    setattr(p, attr, Vector(((v.x - cx) * k, (v.y - cy) * k, 0)))
            for p in s.points:
                p.co = ((p.co.x - cx) * k, (p.co.y - cy) * k, 0, 1)
        o.data.materials.clear()
        o.location = (0, 0, 0)
        o.scale = (1, 1, 1)
        parts.append(dict(meta[o.name.split(".")[0]], obj=o))
    return parts


def setmat(o, m):
    o.data.materials.clear()
    o.data.materials.append(m)


def solid(o, depth, bevel, z0):
    c = o.data
    c.dimensions = "2D"
    c.fill_mode = "BOTH"
    c.extrude = max(depth / 2 - bevel, 0.0005)
    c.bevel_depth = bevel
    c.bevel_resolution = 6
    c.resolution_u = 24
    o.location.z = z0 + c.extrude + bevel


def tube(o, radius, z, m):
    c = o.data
    c.dimensions = "3D"
    c.fill_mode = "FULL"
    c.bevel_mode = "ROUND"
    c.bevel_depth = radius
    c.bevel_resolution = 6
    c.extrude = 0
    c.use_fill_caps = True
    c.resolution_u = 24
    o.location.z = z
    for s in c.splines:
        if s.use_cyclic_u:
            continue
        ends = [s.bezier_points[0].co, s.bezier_points[-1].co] if s.type == "BEZIER" else [s.points[0].co.xyz, s.points[-1].co.xyz]
        for e in ends:
            bpy.ops.mesh.primitive_uv_sphere_add(radius=radius, location=(e.x, e.y, z), segments=24, ring_count=12)
            bpy.ops.object.shade_smooth()
            bpy.context.object.data.materials.append(m)


def font():
    for path in FONTS:
        if os.path.isfile(path):
            return bpy.data.fonts.load(path)
    return None


def word(t, z, soft):
    cu = bpy.data.curves.new("word", "FONT")
    cu.body = t["s"]
    face = font()
    if face:
        cu.font = face
    cu.align_x = "CENTER"
    cu.align_y = "CENTER"
    cu.size = t["h"] / 100 * 1.4
    cu.extrude = 0.01 * soft
    cu.bevel_depth = 0.004 * soft
    cu.bevel_resolution = 3
    cu.resolution_u = 12
    o = bpy.data.objects.new("word", cu)
    bpy.context.collection.objects.link(o)
    bpy.context.view_layer.update()
    width = o.dimensions.x
    limit = t["maxw"] / 100
    if width > limit:
        o.scale = (limit / width, limit / width, 1)
    o.location = ((t["x"] - 50) / 100, (50 - t["y"]) / 100, z + cu.extrude + cu.bevel_depth)
    setmat(o, clay(t["color"]))


def build(key):
    s = SUBJECTS[key]
    soft = s["soft"]
    parts = load(key)
    tb, tt, tm = 0.13 * soft, 0.05 * soft, 0.036 * soft
    bb, bt, bm = 0.05 * soft, 0.022 * soft, 0.016 * soft
    has_top = any(p["role"] == "top" for p in parts)
    z = {"body": 0.0, "top": tb, "mark": tb + (tt if has_top else 0)}
    thick = {"body": tb, "top": tt, "mark": tm}
    bev = {"body": bb, "top": bt, "mark": bm}
    for p in parts:
        o = p["obj"]
        m = clay(p["color"])
        setmat(o, m)
        if p["kind"] == "line":
            r = p["width"] / 200
            tube(o, r, z[p["role"]] + r * 0.9, m)
        else:
            solid(o, thick[p["role"]], bev[p["role"]], z[p["role"]])
    for t in s["texts"]:
        word(t, z[t["role"]], soft)


def camera():
    d = Vector((0.0, -0.08, 1.0)).normalized()
    cam = bpy.data.cameras.new("C")
    o = bpy.data.objects.new("C", cam)
    bpy.context.collection.objects.link(o)
    o.location = Vector((0, 0, 0.04)) + d * 6.0
    o.rotation_euler = (-d).to_track_quat("-Z", "Y").to_euler()
    cam.sensor_width = 36
    cam.lens = 36 * 6.0 / 1.06
    bpy.context.scene.camera = o


def setup():
    sc = bpy.context.scene
    try:
        sc.render.engine = "CYCLES"
    except TypeError:
        pass
    prefs = bpy.context.preferences.addons["cycles"].preferences
    for t in ("OPTIX", "CUDA"):
        try:
            prefs.compute_device_type = t
            prefs.get_devices()
            for dv in prefs.devices:
                dv.use = dv.type == t
            break
        except TypeError:
            continue
    sc.cycles.device = "GPU"
    sc.render.film_transparent = True
    sc.render.resolution_x = SIZE
    sc.render.resolution_y = SIZE
    sc.render.resolution_percentage = 100
    sc.render.image_settings.file_format = "PNG"
    sc.render.image_settings.color_mode = "RGBA"
    w = bpy.data.worlds.new("W")
    sc.world = w
    w.use_nodes = True
    bg = next(n for n in w.node_tree.nodes if n.type == "BACKGROUND")
    bg.inputs["Color"].default_value = (1, 1, 1, 1)
    bg.inputs["Strength"].default_value = 1.0
    sc.view_settings.look = "None"
    sc.view_settings.exposure = 0.0
    return sc, bg


def ambient(sc, path):
    sc.view_settings.view_transform = "Standard"
    sc.render.image_settings.color_depth = "8"
    sc.cycles.samples = SAMPLES
    sc.cycles.use_denoising = True
    sc.render.filepath = path
    bpy.ops.render.render(write_still=True)


def normals(sc, bg, path):
    bg.inputs["Strength"].default_value = 0.0
    m = normal_material()
    for o in bpy.data.objects:
        if o.type in ("CURVE", "MESH", "FONT"):
            o.data.materials.clear()
            o.data.materials.append(m)
    try:
        sc.view_settings.view_transform = "Raw"
    except TypeError:
        sc.view_settings.view_transform = "Standard"
    sc.render.image_settings.color_depth = "16"
    sc.cycles.samples = 16
    sc.cycles.use_denoising = False
    sc.render.filepath = path
    bpy.ops.render.render(write_still=True)


for sub in ("color", "normal"):
    os.makedirs(os.path.join(OUT, sub), exist_ok=True)
for key in SUBJECTS:
    if ONLY and key not in ONLY and SUBJECTS[key]["path"] not in ONLY:
        continue
    build(key)
    camera()
    sc, bg = setup()
    ambient(sc, os.path.join(OUT, "color", f"{key}.png"))
    normals(sc, bg, os.path.join(OUT, "normal", f"{key}.png"))
    print("DONE", key, flush=True)
