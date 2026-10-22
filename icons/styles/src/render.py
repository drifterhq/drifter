import bpy
import json
import math
import os
import sys
from mathutils import Vector

argv = sys.argv[sys.argv.index("--") + 1:]
STYLE = argv[0]
SRC = argv[1]
OUT = argv[2]
ONLY = argv[3].split(",") if len(argv) > 3 and argv[3] else None
SIZE = int(os.environ.get("ICON_SIZE", "384"))
SAMPLES = int(os.environ.get("ICON_SAMPLES", "128"))
FLAT = os.environ.get("ICON_VIEW") == "flat"
PASS = os.environ.get("ICON_PASS", "")


def as_pass():
    sc = bpy.context.scene
    for o in list(bpy.data.objects):
        if o.type == "LIGHT" or o.name == "catcher":
            bpy.data.objects.remove(o)
    w = sc.world
    bg = next(n for n in w.node_tree.nodes if n.type == "BACKGROUND")
    for link in list(bg.inputs["Color"].links):
        w.node_tree.links.remove(link)
    if PASS == "ambient":
        bg.inputs["Color"].default_value = (1, 1, 1, 1)
        bg.inputs["Strength"].default_value = 1.0
        sc.view_settings.exposure = 0.0
        return
    bg.inputs["Strength"].default_value = 0.0
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
    for o in bpy.data.objects:
        if o.type in ("CURVE", "MESH"):
            o.data.materials.clear()
            o.data.materials.append(m)
    try:
        sc.view_settings.view_transform = "Raw"
    except TypeError:
        sc.view_settings.view_transform = "Standard"
    sc.view_settings.exposure = 0.0
    sc.render.image_settings.color_depth = "16"
    sc.cycles.samples = 16
    sc.cycles.use_denoising = False

SUBJECTS = json.load(open(os.path.join(SRC, "subjects.json")))


def lin(h):
    h = h.lstrip("#")
    c = [int(h[i:i + 2], 16) / 255 for i in (0, 2, 4)]
    return tuple(x / 12.92 if x <= 0.04045 else ((x + 0.055) / 1.055) ** 2.4 for x in c)


def mix(a, b, t):
    return tuple(x * (1 - t) + y * t for x, y in zip(a, b))


WHITE = (1.0, 1.0, 1.0)
BLACK = (0.0, 0.0, 0.0)


def principled(name, color, **kw):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    b = next(n for n in m.node_tree.nodes if n.type == "BSDF_PRINCIPLED")
    b.inputs["Base Color"].default_value = (*color, 1)
    keys = {
        "metal": "Metallic", "rough": "Roughness", "ior": "IOR", "glass": "Transmission Weight",
        "coat": "Coat Weight", "coat_rough": "Coat Roughness", "sss": "Subsurface Weight",
        "spec": "Specular IOR Level", "film": "Thin Film Thickness", "sheen": "Sheen Weight",
    }
    for k, v in kw.items():
        if k == "emit":
            b.inputs["Emission Color"].default_value = (*v[0], 1)
            b.inputs["Emission Strength"].default_value = v[1]
        elif k == "sss_radius":
            b.inputs["Subsurface Radius"].default_value = v
        else:
            b.inputs[keys[k]].default_value = v
    return m


def emitter(name, color, strength):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nt = m.node_tree
    for n in list(nt.nodes):
        nt.nodes.remove(n)
    out = nt.nodes.new("ShaderNodeOutputMaterial")
    e = nt.nodes.new("ShaderNodeEmission")
    e.inputs["Color"].default_value = (*color, 1)
    e.inputs["Strength"].default_value = strength
    nt.links.new(e.outputs[0], out.inputs[0])
    return m


def load(name):
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_curve.svg(filepath=os.path.join(SRC, f"{name}.blend.svg"))
    ref = bpy.data.objects["zzref"]
    pts = [p.co for s in ref.data.splines for p in (s.bezier_points if s.type == "BEZIER" else s.points)]
    x0 = min(p.x for p in pts)
    x1 = max(p.x for p in pts)
    y0 = min(p.y for p in pts)
    y1 = max(p.y for p in pts)
    k = 1.0 / (x1 - x0)
    cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
    bpy.data.objects.remove(ref)
    parts = {}
    meta = {p["id"]: p for p in SUBJECTS[name]["parts"]}
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
        base = o.name.split(".")[0]
        parts[o.name] = dict(meta[base], obj=o)
    return parts


def has_top(parts):
    return any(p["role"] == "top" for p in parts.values())


def solid(o, depth, bevel, z0, offset=0.0, res=6):
    c = o.data
    c.dimensions = "2D"
    c.fill_mode = "BOTH"
    c.extrude = max(depth / 2 - bevel, 0.0005)
    c.bevel_depth = bevel
    c.bevel_resolution = res
    c.offset = offset - 0.0 if bevel == 0 else offset
    c.resolution_u = 24
    o.location.z = z0 + c.extrude + bevel
    return o


def tube(o, radius, z, caps=True):
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
    if caps:
        for s in c.splines:
            if s.use_cyclic_u:
                continue
            ends = [s.bezier_points[0].co, s.bezier_points[-1].co] if s.type == "BEZIER" else [s.points[0].co.xyz, s.points[-1].co.xyz]
            for e in ends:
                bpy.ops.mesh.primitive_uv_sphere_add(radius=radius, location=(e.x, e.y, z), segments=24, ring_count=12)
                sp = bpy.context.object
                bpy.ops.object.shade_smooth()
                sp.data.materials.append(o.data.materials[0] if o.data.materials else None)
                sp["cap_of"] = o.name
    return o


def copy(o, suffix):
    n = o.copy()
    n.data = o.data.copy()
    n.name = o.name + suffix
    bpy.context.collection.objects.link(n)
    return n


def setmat(o, m):
    o.data.materials.clear()
    o.data.materials.append(m)
    for c in bpy.data.objects:
        if c.get("cap_of") == o.name:
            c.data.materials.clear()
            c.data.materials.append(m)


def world(strength, bright=1.0):
    w = bpy.data.worlds.new("W")
    bpy.context.scene.world = w
    w.use_nodes = True
    nt = w.node_tree
    bg = next(n for n in nt.nodes if n.type == "BACKGROUND")
    tc = nt.nodes.new("ShaderNodeTexCoord")
    sep = nt.nodes.new("ShaderNodeSeparateXYZ")
    ramp = nt.nodes.new("ShaderNodeValToRGB")
    nt.links.new(tc.outputs["Generated"], sep.inputs[0])
    nt.links.new(sep.outputs["Z"], ramp.inputs[0])
    stops = [
        (0.0, "#0a0f24"), (0.18, "#2a1f6a"), (0.32, "#1b6f6a"), (0.46, "#0b1026"),
        (0.6, "#2fbf8f"), (0.72, "#6a4de0"), (0.86, "#151b3a"), (1.0, "#dfe6ff"),
    ]
    cr = ramp.color_ramp
    cr.elements[0].position = stops[0][0]
    cr.elements[0].color = (*lin(stops[0][1]), 1)
    cr.elements[1].position = stops[-1][0]
    cr.elements[1].color = (*mix(lin(stops[-1][1]), WHITE, 0), 1)
    for pos, col in stops[1:-1]:
        e = cr.elements.new(pos)
        e.color = (*lin(col), 1)
    scale = nt.nodes.new("ShaderNodeMath")
    scale.operation = "ADD"
    nt.links.new(sep.outputs["Z"], scale.inputs[0])
    scale.inputs[1].default_value = 0
    nt.links.new(sep.outputs["Z"], ramp.inputs[0])
    tc2 = nt.nodes.new("ShaderNodeMapRange")
    tc2.inputs["From Min"].default_value = -1
    tc2.inputs["From Max"].default_value = 1
    nt.links.new(sep.outputs["Z"], tc2.inputs["Value"])
    nt.links.new(tc2.outputs[0], ramp.inputs[0])
    nt.links.new(ramp.outputs[0], bg.inputs["Color"])
    bg.inputs["Strength"].default_value = strength
    tc.object = None
    w.cycles_visibility.camera = True


def area(name, loc, size, energy, color=WHITE, target=(0, 0, 0)):
    d = bpy.data.lights.new(name, "AREA")
    d.shape = "DISK"
    d.size = size
    d.energy = energy * 0.22
    d.color = color
    o = bpy.data.objects.new(name, d)
    bpy.context.collection.objects.link(o)
    o.location = loc
    direction = Vector(target) - Vector(loc)
    o.rotation_euler = direction.to_track_quat("-Z", "Y").to_euler()
    return o


def catcher(z, size=4):
    bpy.ops.mesh.primitive_plane_add(size=size, location=(0, 0, z))
    p = bpy.context.object
    p.is_shadow_catcher = True
    p.name = "catcher"
    return p


def floor(z=-0.001, size=4):
    bpy.ops.mesh.primitive_plane_add(size=size, location=(0, 0, z))
    p = bpy.context.object
    p.name = "floor"
    p.visible_camera = False
    p.visible_shadow = False
    p.data.materials.append(principled("floor", lin("#c9d2e6"), rough=0.6))
    return p


def camera(direction=(0.12, -0.42, 1.0), width=1.44, ortho=False, target=(0, 0, 0.04)):
    if FLAT and not ortho:
        direction = (0.0, -0.08, 1.0)
        width = 1.3
    d = Vector(direction).normalized()
    cam = bpy.data.cameras.new("C")
    o = bpy.data.objects.new("C", cam)
    bpy.context.collection.objects.link(o)
    dist = 6.0
    o.location = Vector(target) + d * dist
    o.rotation_euler = (-d).to_track_quat("-Z", "Y").to_euler()
    if ortho:
        cam.type = "ORTHO"
        cam.ortho_scale = width
    else:
        cam.sensor_width = 36
        cam.lens = 36 * dist / width
    bpy.context.scene.camera = o


def render_setup(transparent_glass=True, exposure=0.0):
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
    sc.cycles.samples = SAMPLES
    sc.cycles.use_denoising = True
    try:
        sc.cycles.denoiser = "OPENIMAGEDENOISE"
    except TypeError:
        pass
    sc.cycles.max_bounces = 16
    sc.cycles.transmission_bounces = 16
    sc.cycles.glossy_bounces = 8
    sc.cycles.transparent_max_bounces = 16
    sc.render.film_transparent = True
    sc.cycles.film_transparent_glass = transparent_glass
    sc.render.resolution_x = SIZE
    sc.render.resolution_y = SIZE
    sc.render.resolution_percentage = 100
    sc.render.image_settings.file_format = "PNG"
    sc.render.image_settings.color_mode = "RGBA"
    sc.render.image_settings.color_depth = "8"
    try:
        sc.view_settings.view_transform = "Standard"
    except TypeError:
        pass
    sc.view_settings.look = "None"
    sc.view_settings.exposure = exposure


def lights_studio(key=900, rim=500, fill=150, tint=WHITE):
    area("key", (-1.8, -1.2, 3.2), 2.2, key)
    area("rim", (2.0, 2.2, 1.6), 1.6, rim, tint)
    area("fill", (2.2, -2.4, 1.2), 2.5, fill)


def color_of(p):
    return lin(p["color"])


def stack_z(parts, tb, tt):
    top0 = tb
    mark0 = tb + (tt if has_top(parts) else 0)
    return {"body": 0.0, "top": top0, "mark": mark0}


def build_layers(parts, mats, tb, tt, tm, bb, bt, bm, line_scale=1.0):
    z = stack_z(parts, tb, tt)
    thick = {"body": tb, "top": tt, "mark": tm}
    bev = {"body": bb, "top": bt, "mark": bm}
    for name, p in parts.items():
        o = p["obj"]
        m = mats(p)
        setmat(o, m)
        if p["kind"] == "line":
            r = p["width"] / 200 * line_scale
            tube(o, r, z[p["role"]] + r * 0.9)
            setmat(o, m)
        else:
            solid(o, thick[p["role"]], bev[p["role"]], z[p["role"]])


def style_glass(parts):
    def mats(p):
        c = color_of(p)
        if p["role"] == "mark":
            return principled("m", mix(c, WHITE, 0.15), glass=0.0, rough=0.25, coat=1.0, coat_rough=0.05, emit=(c, 0.35))
        tint = mix(c, WHITE, 0.12 if p["role"] == "body" else 0.05)
        return principled("g", tint, glass=1.0, rough=0.02, ior=1.45)
    build_layers(parts, mats, 0.13, 0.03, 0.02, 0.06, 0.012, 0.008)
    world(1.6)
    lights_studio(700, 700, 120)
    floor()
    camera()
    render_setup(transparent_glass=False)


def style_panes(parts):
    for name, p in list(parts.items()):
        o = p["obj"]
        c = color_of(p)
        if p["role"] == "body":
            setmat(o, principled("back", c, rough=0.3, coat=0.6, coat_rough=0.1))
            if p["kind"] == "line":
                tube(o, p["width"] / 200, 0.02)
                setmat(o, o.data.materials[0])
            else:
                solid(o, 0.035, 0.008, 0.0)
            f = copy(o, "_frost")
            setmat(f, principled("frost", mix(c, WHITE, 0.9), glass=1.0, rough=0.4, ior=1.45))
            if p["kind"] != "line":
                solid(f, 0.03, 0.008, 0.12)
            f.location.x += 0.09
            f.location.y -= 0.09
        else:
            hue = lin(SUBJECTS[CURRENT]["hue"])
            ink = mix(hue, BLACK, 0.25) if p["color"].lower() in ("#f4f6fb", "#fff0c8", "#c9c2b6") else c
            m = principled("lit", ink, rough=0.25, coat=1.0, emit=(ink, 0.25))
            setmat(o, m)
            if p["kind"] == "line":
                r = p["width"] / 220
                tube(o, r, 0.155 + r)
                setmat(o, m)
            else:
                solid(o, 0.012 if p["role"] == "top" else 0.01, 0.003, 0.152 if p["role"] == "top" else 0.162)
            o.location.x += 0.09
            o.location.y -= 0.09
            for cap in [x for x in bpy.data.objects if x.get("cap_of") == o.name]:
                cap.location.x += 0.09
                cap.location.y -= 0.09
    world(1.0)
    lights_studio(600, 500, 120)
    floor()
    camera()
    render_setup(transparent_glass=False, exposure=-0.3)


def style_clay(parts):
    def mats(p):
        c = mix(color_of(p), WHITE, 0.08)
        if p["role"] == "mark" and p["color"].lower() in ("#f4f6fb", "#fff0c8"):
            c = lin("#fbf6ee")
        return principled("clay", c, rough=0.62, sss=0.12, sss_radius=(0.05, 0.03, 0.02), spec=0.3)
    build_layers(parts, mats, 0.13, 0.05, 0.036, 0.05, 0.022, 0.016)
    world(0.35)
    area("key", (-1.4, -1.6, 3.0), 3.4, 900)
    area("fill", (2.0, -1.4, 2.0), 3.0, 260)
    area("top", (0, 0.8, 3.2), 3.0, 200)
    catcher(-0.001)
    camera()
    render_setup(exposure=-0.7)


def style_enamel(parts):
    silver = principled("metal", lin("#dfe3ec"), metal=1.0, rough=0.16)
    for name, p in list(parts.items()):
        o = p["obj"]
        c = color_of(p)
        enamel = principled("enamel", c, rough=0.08, coat=1.0, coat_rough=0.02)
        role = p["role"]
        base = {"body": 0.0, "top": 0.05, "mark": 0.07 if has_top(parts) else 0.05}[role]
        if p["kind"] == "line":
            setmat(o, silver)
            r = p["width"] / 220
            tube(o, r, base + r * 0.7)
            setmat(o, silver)
            continue
        if role == "mark" and p["color"].lower() in ("#f4f6fb", "#fff0c8", "#c9c2b6"):
            setmat(o, silver)
            solid(o, 0.022, 0.006, base)
            continue
        rim = copy(o, "_rim")
        setmat(rim, silver)
        th = 0.04 if role == "body" else 0.016
        solid(rim, th, 0.006 if role == "body" else 0.003, base, offset=0.014 if role == "body" else 0.006)
        setmat(o, enamel)
        solid(o, 0.01, 0.0, base + th - 0.004)
        wire = copy(o, "_wire")
        setmat(wire, silver)
        tube(wire, 0.0055 if role == "body" else 0.004, base + th + 0.006, caps=False)
        setmat(wire, silver)
    world(0.8)
    lights_studio(900, 600, 220)
    catcher(-0.001)
    camera()
    render_setup()


def style_neon(parts):
    for name, p in list(parts.items()):
        o = p["obj"]
        c = color_of(p)
        if sum(c) < 0.15 or p["color"].lower() in ("#2a2f40", "#4a5868", "#5a3d0a"):
            c = lin("#9aaabb")
        hot = mix(c, WHITE, 0.25)
        m = emitter("neon", hot, 1.0)
        setmat(o, m)
        z = {"body": 0.0, "top": 0.012, "mark": 0.024}[p["role"]]
        tube(o, 0.015 if p["kind"] != "line" else 0.017, z + 0.016, caps=p["kind"] == "line")
        setmat(o, m)
    world(0.0)
    camera(direction=(0.0, -0.18, 1.0))
    render_setup()


def style_iso(parts):
    def mats(p):
        return principled("satin", color_of(p), rough=0.38, coat=0.3, coat_rough=0.2)
    build_layers(parts, mats, 0.22, 0.05, 0.04, 0.012, 0.006, 0.006)
    world(0.25)
    sun = bpy.data.lights.new("sun", "SUN")
    sun.energy = 4.2
    sun.angle = math.radians(12)
    so = bpy.data.objects.new("sun", sun)
    bpy.context.collection.objects.link(so)
    so.rotation_euler = (math.radians(18), math.radians(-10), 0)
    area("fill", (2.5, -2.5, 1.0), 3.0, 120)
    catcher(-0.001)
    camera(direction=(0.62, -0.62, 1.0), width=1.62, ortho=True, target=(0, 0, 0.1))
    render_setup()


def style_topo(parts):
    for name, p in list(parts.items()):
        o = p["obj"]
        c = color_of(p)
        role = p["role"]
        if p["kind"] == "line":
            m = principled("t", mix(c, WHITE, 0.1), rough=0.5)
            setmat(o, m)
            tube(o, p["width"] / 220, (0.15 if has_top(parts) else 0.108) + p["width"] / 220)
            setmat(o, m)
            continue
        if role == "mark":
            m = principled("t", mix(c, WHITE, 0.1), rough=0.45)
            setmat(o, m)
            solid(o, 0.016, 0.003, 0.15 if has_top(parts) else 0.108)
            continue
        steps = 5 if role == "body" else 2
        z0 = 0.0 if role == "body" else 0.108
        for i in range(steps):
            layer = o if i == steps - 1 else copy(o, f"_l{i}")
            t = i / max(steps - 1, 1)
            col = mix(mix(c, BLACK, 0.6), mix(c, WHITE, 0.2), t)
            setmat(layer, principled("t", col, rough=0.55))
            solid(layer, 0.02, 0.003, z0 + i * 0.022, offset=0.016 * (steps - 1 - i) if role == "body" else 0.008 * (steps - 1 - i))
    world(0.35)
    area("key", (-2.2, -0.6, 2.2), 1.6, 1100)
    area("fill", (2.0, -1.4, 2.4), 3.0, 200)
    catcher(-0.001)
    camera(direction=(0.18, -0.5, 1.0))
    render_setup(exposure=-0.6)


def style_smoked(parts):
    for name, p in list(parts.items()):
        o = p["obj"]
        c = color_of(p)
        glow = mix(c, WHITE, 0.15)
        if sum(c) < 0.15:
            glow = lin("#7a8ca0")
        role = p["role"]
        if p["kind"] == "line" or role == "mark":
            m = emitter("e", mix(glow, WHITE, 0.35), 4.0)
            setmat(o, m)
            if p["kind"] == "line":
                r = p["width"] / 240
                tube(o, r, 0.1 + r)
                setmat(o, m)
            else:
                solid(o, 0.01, 0.002, 0.1 if not has_top(parts) else 0.125)
            continue
        dark = principled("smoke", mix(lin("#1a1d28"), c, 0.12), glass=1.0, rough=0.06, ior=1.5)
        setmat(o, dark)
        th = 0.1 if role == "body" else 0.025
        z0 = 0.0 if role == "body" else 0.1
        solid(o, th, 0.012 if role == "body" else 0.006, z0)
        edge = copy(o, "_edge")
        e = emitter("edge", glow, 6.0)
        setmat(edge, e)
        tube(edge, 0.0045, z0 + th - 0.006, caps=False)
        setmat(edge, e)
    world(0.9)
    lights_studio(500, 900, 80)
    floor()
    camera()
    render_setup(transparent_glass=False)


def style_porcelain(parts):
    china = principled("china", lin("#f6f3ee"), rough=0.3, coat=1.0, coat_rough=0.03, sss=0.06, sss_radius=(0.02, 0.02, 0.02))
    for name, p in list(parts.items()):
        o = p["obj"]
        c = color_of(p)
        role = p["role"]
        glaze = principled("glaze", c, rough=0.12, coat=1.0, coat_rough=0.02)
        if p["color"].lower() in ("#f3efe8", "#eef1f7", "#f4f6fb", "#c9c2b6", "#fff0c8"):
            glaze = china if p["color"].lower() != "#c9c2b6" else principled("g2", lin("#d9d3c8"), rough=0.2, coat=1.0)
        if p["kind"] == "line":
            setmat(o, glaze)
            r = p["width"] / 220
            tube(o, r, {"body": 0.06, "top": 0.08, "mark": 0.09}[role] + r * 0.6)
            setmat(o, glaze)
            continue
        if role == "body":
            setmat(o, china)
            solid(o, 0.07, 0.025, 0.0)
            if glaze is not china:
                g = copy(o, "_glaze")
                setmat(g, glaze)
                solid(g, 0.02, 0.006, 0.058, offset=-0.014)
        else:
            setmat(o, glaze)
            z0 = 0.07 if role == "top" else (0.09 if has_top(parts) else 0.075)
            solid(o, 0.022 if role == "top" else 0.016, 0.006, z0)
    world(0.6)
    lights_studio(900, 450, 200)
    catcher(-0.001)
    camera()
    render_setup()


STYLES = {
    "glass": style_glass, "panes": style_panes, "clay": style_clay, "enamel": style_enamel,
    "neon": style_neon, "iso": style_iso, "topo": style_topo, "smoked": style_smoked,
    "porcelain": style_porcelain,
}

os.makedirs(os.path.join(OUT, STYLE), exist_ok=True)
for name in SUBJECTS:
    if ONLY and name not in ONLY:
        continue
    CURRENT = name
    parts = load(name)
    STYLES[STYLE](parts)
    if PASS:
        as_pass()
    bpy.context.scene.render.filepath = os.path.join(OUT, STYLE, f"{name}.png")
    bpy.ops.render.render(write_still=True)
    print("DONE", STYLE, name, flush=True)
