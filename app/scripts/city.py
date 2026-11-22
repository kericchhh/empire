"""Renders the home screen's backdrop, app/src/city.webp: a made-up ancient metropolis (Rome, Greece
and Alexandria mixed) at sunset, modelled in Blender and rendered at full HD.
    blender -b -P app/scripts/city.py
Deterministic. Geometry is merged per material, so thousands of buildings stay fast."""

import math
import random
from pathlib import Path

import bpy

OUT = Path(__file__).resolve().parent.parent / "src" / "city.webp"
R = random.Random(7)
bpy.ops.wm.read_factory_settings(use_empty=True)
scene = bpy.context.scene


def lin(hex_):
    c = [int(hex_[i : i + 2], 16) / 255 for i in (1, 3, 5)]
    return [x / 12.92 if x <= 0.04045 else ((x + 0.055) / 1.055) ** 2.4 for x in c] + [1]


def material(color, rough=0.85, noise=None, emit=0.0):
    """A flat colour, optionally broken up by world-space noise into a second colour."""
    m = bpy.data.materials.new(color)
    nodes, links = m.node_tree.nodes, m.node_tree.links
    bsdf = nodes["Principled BSDF"]
    bsdf.inputs["Roughness"].default_value = rough
    bsdf.inputs["Base Color"].default_value = lin(color)
    if emit:
        bsdf.inputs["Emission Color"].default_value = lin(color)
        bsdf.inputs["Emission Strength"].default_value = emit
    if noise:
        color2, scale = noise
        tex = nodes.new("ShaderNodeTexNoise")
        tex.inputs["Scale"].default_value = scale
        links.new(nodes.new("ShaderNodeNewGeometry").outputs["Position"], tex.inputs["Vector"])
        mix = nodes.new("ShaderNodeMix")
        mix.data_type = "RGBA"
        mix.inputs["A"].default_value, mix.inputs["B"].default_value = lin(color), lin(color2)
        links.new(tex.outputs["Fac"], mix.inputs["Factor"])
        links.new(mix.outputs["Result"], bsdf.inputs["Base Color"])
    return m


M = {
    "grass": material("#3d6e2a", noise=("#5d8f38", 0.08)),
    "field": material("#6f8a3a", noise=("#8a9a48", 0.3)),
    "wheat": material("#b89a4a", noise=("#cfae5a", 0.3)),
    "street": material("#9d968a", noise=("#7f786d", 0.4)),
    "pave": material("#cbc2ad", noise=("#b0a690", 0.5)),
    "sand": material("#cdb27a"),
    "marble": material("#efe8d6", 0.5),
    "stone": material("#d6cbb1"),
    "wall": material("#efdfbd"),
    "wall2": material("#e2c9a0"),
    "tile": material("#c4572e", noise=("#a2421f", 1.5)),
    "tile2": material("#b0603a"),
    "dark": material("#2d221b"),
    "lamp": material("#ffbe55", emit=7.0),
    "water": material("#1d5f7a", 0.05, noise=("#2a7f98", 0.06)),
    "leaf": material("#2d6a2a", noise=("#477f33", 1.2)),
    "cypress": material("#1d4a24"),
    "autumn": material("#b65a23"),
    "purple": material("#6a2c74"),
    "wheat2": material("#a4863c"),
    "crop": material("#5f7f32"),
    "crop2": material("#4f6e2a"),
    "soil": material("#6b5235"),
    "dirt": material("#8a7350"),
    "vine": material("#2f5a26"),
    "hull": material("#4a2f1c"),
    "deck": material("#8a6238"),
    "sail": material("#eadfc4"),
    "sail2": material("#a8382a"),
}
LIGHTS = []  # (x, y, z, watts): real light where the scene shows a flame


def light(x, y, z, watts=400):
    LIGHTS.append((x, y, z, watts))
GEO = {k: ([], []) for k in M}


def mesh(mat, verts, faces):
    v, f = GEO[mat]
    o = len(v)
    v.extend(verts)
    f.extend(tuple(i + o for i in face) for face in faces)


def box(x, y, z, sx, sy, sz, mat, rot=0.0):
    c, s = math.cos(rot), math.sin(rot)
    pts = [(-sx / 2, -sy / 2), (sx / 2, -sy / 2), (sx / 2, sy / 2), (-sx / 2, sy / 2)]
    ring = [(x + px * c - py * s, y + px * s + py * c) for px, py in pts]
    verts = [(a, b, z) for a, b in ring] + [(a, b, z + sz) for a, b in ring]
    mesh(mat, verts, [(0, 3, 2, 1), (4, 5, 6, 7), (0, 1, 5, 4), (1, 2, 6, 5), (2, 3, 7, 6), (3, 0, 4, 7)])


def cyl(x, y, z, rx, h, mat, n=10, ry=None, r2=None):
    """A cylinder, ellipse-based with `ry`, or a cone or frustum with top radius `r2`."""
    ry = ry or rx
    k = 1 if r2 is None else r2 / rx
    ring = [(math.cos(i * math.tau / n), math.sin(i * math.tau / n)) for i in range(n)]
    verts = [(x + a * rx, y + b * ry, z) for a, b in ring] + [(x + a * rx * k, y + b * ry * k, z + h) for a, b in ring]
    faces = [(i, (i + 1) % n, n + (i + 1) % n, n + i) for i in range(n)]
    mesh(mat, verts, faces + [tuple(range(n - 1, -1, -1)), tuple(range(n, 2 * n))])


ICO_V = [(-1, 1.618, 0), (1, 1.618, 0), (-1, -1.618, 0), (1, -1.618, 0), (0, -1, 1.618), (0, 1, 1.618),
         (0, -1, -1.618), (0, 1, -1.618), (1.618, 0, -1), (1.618, 0, 1), (-1.618, 0, -1), (-1.618, 0, 1)]
ICO_F = [(0, 11, 5), (0, 5, 1), (0, 1, 7), (0, 7, 10), (0, 10, 11), (1, 5, 9), (5, 11, 4), (11, 10, 2), (10, 7, 6),
         (7, 1, 8), (3, 9, 4), (3, 4, 2), (3, 2, 6), (3, 6, 8), (3, 8, 9), (4, 9, 5), (2, 4, 11), (6, 2, 10),
         (8, 6, 7), (9, 8, 1)]


def blob(x, y, z, r, mat, sz=1.0):
    k = r / 1.902
    mesh(mat, [(x + a * k, y + b * k, z + c * k * sz) for a, b, c in ICO_V], ICO_F)


def roof(x, y, z, sx, sy, h, mat="tile", hip=True, end=None):
    """A hipped roof, or a gable whose triangular ends (facing the camera) use `end`."""
    along_x = sx >= sy if hip else False
    hx, hy = sx / 2, sy / 2
    inset = min(sx, sy) / 2 if hip else 0
    ridge = [(x - hx + inset, y, z + h), (x + hx - inset, y, z + h)] if along_x else \
            [(x, y - hy + inset, z + h), (x, y + hy - inset, z + h)]
    base = [(x - hx, y - hy, z), (x + hx, y - hy, z), (x + hx, y + hy, z), (x - hx, y + hy, z)]
    if along_x:
        slopes, ends = [(0, 1, 5, 4), (2, 3, 4, 5)], [(3, 0, 4), (1, 2, 5)]
    else:
        slopes, ends = [(1, 2, 5, 4), (3, 0, 4, 5)], [(0, 1, 4), (2, 3, 5)]
    mesh(mat, base + ridge, slopes + (ends if end is None else []))
    if end:
        mesh(end, base + ridge, ends)


def ribbon(mat, left, right, z, ys):
    """A strip between two curves of x over y: the river, its banks, the sea."""
    verts = [(left(y), y, z) for y in ys] + [(right(y), y, z) for y in ys]
    n = len(ys)
    mesh(mat, verts, [(i, n + i, n + i + 1, i + 1) for i in range(n - 1)])


def dome(x, y, z, r, mat, sz=0.85, n=20, rings=6):
    verts = [(x + r * math.cos(a) * math.cos(t), y + r * math.sin(a) * math.cos(t), z + r * sz * math.sin(t))
             for t in (k * math.pi / 2 / rings for k in range(rings)) for a in (i * math.tau / n for i in range(n))]
    faces = [(k * n + i, k * n + (i + 1) % n, (k + 1) * n + (i + 1) % n, (k + 1) * n + i)
             for k in range(rings - 1) for i in range(n)]
    verts.append((x, y, z + r * sz))
    faces += [((rings - 1) * n + i, (rings - 1) * n + (i + 1) % n, len(verts) - 1) for i in range(n)]
    mesh(mat, verts, faces)


def ship(x, y, length, rot, sailing=False):
    """A merchant ship: a pointed hull with a deck, a mast, and a sail set or furled."""
    w = length * 0.3
    c, s = math.cos(rot), math.sin(rot)
    deck = [(length / 2, 0), (length / 5, w / 2), (-length / 2, w * 0.4), (-length / 2, -w * 0.4), (length / 5, -w / 2)]
    pt = lambda px, py, z, k=1.0: (x + (px * c - py * s) * k, y + (px * s + py * c) * k, z)
    verts = [pt(px, py, 0, 0.8) for px, py in deck] + [pt(px, py, 1.6) for px, py in deck]
    mesh("hull", verts, [(i, (i + 1) % 5, 5 + (i + 1) % 5, 5 + i) for i in range(5)] + [(4, 3, 2, 1, 0)])
    mesh("deck", [pt(px, py, 1.6, 0.9) for px, py in deck], [(0, 1, 2, 3, 4)])
    cyl(x, y, 1.6, 0.18, length * 0.7, "deck", 4)
    if sailing:
        box(x, y, 1.6 + length * 0.2, 0.15, length * 0.42, length * 0.42, R.choice(("sail", "sail", "sail2")), rot)
    else:
        box(x, y, 1.6 + length * 0.55, 0.3, length * 0.42, 0.4, "sail", rot)
    if R.random() < 0.5:
        box(*pt(-length / 2 + 0.6, 0, 1.6)[:2], 1.6, 0.3, 0.3, 1.0, "lamp")


def tree(x, y, s=1.0, leaf="leaf"):
    cyl(x, y, 0, 0.3 * s, 2 * s, "dark", 5)
    for _ in range(3):
        blob(x + R.uniform(-1, 1) * s, y + R.uniform(-1, 1) * s, (3 + R.uniform(0, 1)) * s, 1.8 * s, leaf)


def cypress(x, y, s=1.0):
    blob(x, y, 3 * s, 1.0 * s, "cypress", sz=3)


# ---- Geography: the walls, a river, the sea ----
WX0, WX1, WY0, WY1 = -250, 215, -230, 235
river = lambda y: -118 + 34 * math.sin(y / 85)
coast = lambda y: 238 + 16 * math.sin(y / 55) + 6 * math.sin(y / 17)
reserved = []  # landmark footprints that the street grid leaves free


def reserve(x, y, w, d):
    reserved.append((x - w / 2, y - d / 2, x + w / 2, y + d / 2))


def free(x0, y0, x1, y1):
    if any(x0 < b[2] and x1 > b[0] and y0 < b[3] and y1 > b[1] for b in reserved):
        return False
    return all(abs((x0 + x1) / 2 - river(y)) > 14 + (x1 - x0) / 2 for y in (y0, (y0 + y1) / 2, y1))


box(0, 0, -0.5, 1400, 1000, 0.5, "grass")
YS = [y * 2 for y in range(-250, 251)]
ribbon("street", lambda y: WX0, coast, 0.08, [y for y in YS if WY0 <= y <= WY1])
ribbon("stone", lambda y: river(y) - 13, lambda y: river(y) + 13, 0.1, YS)
ribbon("water", lambda y: river(y) - 10, lambda y: river(y) + 10, 0.14, YS)
ribbon("sand", lambda y: coast(y) - 7, coast, 0.12, YS)
ribbon("water", coast, lambda y: 900, 0.16, YS)

# ---- Landmarks ----
def temple(x, y, w, d, z=0.0):
    box(x, y, z, w + 2, d + 2, 1.2, "marble")
    box(x, y + 1, z + 1.2, w - 4, d - 5, 6, "stone")
    n = max(3, int(w // 2.6))
    for i in range(n + 1):
        cx = x - w / 2 + 0.8 + i * (w - 1.6) / n
        for cy in (y - d / 2 + 0.8, y + d / 2 - 0.8):
            cyl(cx, cy, z + 1.2, 0.45, 6, "marble", 6)
    box(x, y, z + 7.2, w + 0.4, d + 0.4, 1.1, "marble")
    roof(x, y, z + 8.3, w + 0.6, d + 0.6, w * 0.17, hip=False, end="marble")
    reserve(x, y, w + 6, d + 6)


def colosseum(x, y, a, b):
    n = 56
    for i in range(n):
        t = i * math.tau / n
        px, py = x + math.cos(t) * a, y + math.sin(t) * b
        rot = math.atan2(b * math.cos(t), -a * math.sin(t))
        box(px, py, 0, math.tau * (a + b) / 2 / n + 0.4, 7, 15, "stone", rot)
        for k in range(3):
            box(px - math.sin(rot) * -3.6, py + math.cos(rot) * -3.6, 2 + k * 4.4, 1.6, 0.3, 2.6, "dark", rot)
    cyl(x, y, 0, a - 3.5, 11, "wall2", 32, ry=b - 3.5, r2=a - 14)
    cyl(x, y, 0.1, a - 14, 0.4, "sand", 32, ry=(a - 14) * b / a)
    reserve(x, y, 2 * a + 10, 2 * b + 10)


def circus(x, y, w, d):
    box(x, y, 0, w + 10, d + 10, 6, "stone")
    box(x, y, 0, w, d, 6.2, "sand")
    box(x, y, 6.2, w * 0.7, 2.4, 0.8, "marble")
    for ax in range(int(-w / 2), int(w / 2), 4):
        for side in (-1, 1):
            box(x + ax, y + side * (d / 2 + 5.05), 1, 1.8, 0.2, 3.4, "dark")
    for side in (-1, 1):
        for ay in range(int(-d / 2), int(d / 2), 4):
            box(x + side * (w / 2 + 5.05), y + ay, 1, 0.2, 1.8, 3.4, "dark")
    for ex in (x - w * 0.35, x + w * 0.35):
        cyl(ex, y, 6.2, 1.2, 4, "marble", 6, r2=0.2)
    reserve(x, y, w + 14, d + 14)


def theatre(x, y, r):
    for k in range(7):
        rr = r - k * 2.6
        for i in range(18):
            t = math.pi * (i + 0.5) / 18
            box(x + math.cos(t) * rr, y + math.sin(t) * rr, 0, 2 * rr * math.pi / 18 + 0.3, 2.8, 7 - k, "stone", t + math.pi / 2)
    box(x, y - 3, 0, 2 * r, 6, 9, "wall2")
    roof(x, y - 3, 9, 2 * r + 1, 7, 2.5)
    reserve(x, y, 2 * r + 8, r + 12)


def domed(x, y, r, porch=True):
    cyl(x, y, 0, r, 9, "wall", 20)
    dome(x, y, 9, r * 1.02, "stone")
    cyl(x, y, 9 + r * 0.8, 1.2, 1.4, "marble", 8)
    if porch:
        box(x, y - r - 4, 0, r * 1.3, 8, 1, "marble")
        for i in range(5):
            cyl(x - r * 0.55 + i * r * 0.275, y - r - 7, 1, 0.45, 6, "marble", 6)
        roof(x, y - r - 4, 7, r * 1.4, 8.4, 2.6, hip=False, end="marble")
    reserve(x, y - 2, 2 * r + 6, 2 * r + 14)


def hill(x, y, w, d, h, top=0.6):
    """A hill with a flat summit `h` high, so what stands on it sits on solid ground."""
    cyl(x, y, 0, w / 2, h, "grass", 28, ry=d / 2, r2=w / 2 * top)
    for _ in range(18):
        a, k = R.uniform(0, math.tau), R.uniform(1.04, 1.15)
        tree(x + math.cos(a) * w / 2 * k, y + math.sin(a) * d / 2 * k, 1.1)
    reserve(x, y, w, d)


def lighthouse(x, y):
    blob(x, y, -4, 26, "sand", sz=0.3)
    blob(x, y, -3, 22, "grass", sz=0.3)
    box(x, y, 2, 14, 14, 18, "marble")
    box(x, y, 20, 9, 9, 16, "marble")
    cyl(x, y, 36, 3.5, 10, "marble", 8)
    blob(x, y, 48, 2.6, "lamp")
    light(x, y, 52, 60000)


temple(-205, 170, 20, 30, z=14)
hill(-205, 172, 70, 64, 14, top=0.68)
temple(135, 192, 16, 24, z=12)
for dx in (-24, 24):
    box(135 + dx, 192, 12, 14, 26, 7, "wall")
    roof(135 + dx, 192, 19, 15, 27, 3.5)
hill(135, 192, 96, 56, 12, top=0.78)
colosseum(-207, -96, 30, 24)
circus(115, -188, 120, 24)
theatre(170, 44, 26)
domed(-48, 142, 14)
domed(196, 112, 9, porch=False)
box(-150, 58, 0, 66, 40, 0.3, "pave")
temple(-168, 66, 12, 18)
temple(-132, 66, 12, 18)
box(-150, 44, 0, 50, 9, 8, "wall")
roof(-150, 44, 8, 51, 10, 3)
cyl(-150, 56, 0.3, 0.9, 16, "marble", 8)
reserve(-150, 58, 66, 40)
box(0, 0, 0, 44, 32, 0.25, "pave")
cyl(0, 0, 0.25, 1.6, 20, "stone", 4, r2=0.4)
for fx, fy in ((-14, -8), (14, -8)):
    cyl(fx, fy, 0.25, 2.8, 0.8, "marble", 12)
    cyl(fx, fy, 0.9, 2.3, 0.2, "water", 12)
reserve(0, 0, 44, 32)
lighthouse(262, 150)
# The harbour: two moles closing a basin, quays and piers, warehouses, ships everywhere.
HY0, HY1, HX = -130, 120, 330
box((coast(HY1) + HX) / 2, HY1, 0.2, HX - coast(HY1), 7, 2, "stone")
box((coast(HY0) + HX) / 2, HY0, 0.2, HX - coast(HY0), 7, 2, "stone")
box(HX, HY1 - 50, 0.2, 7, 100, 2, "stone")
box(HX, HY0 + 60, 0.2, 7, 120, 2, "stone")
for my in range(HY0, HY1 + 1, 25):
    light(HX, my, 4, 900)
    box(HX, my, 2.2, 0.6, 0.6, 2, "lamp")
for py in range(HY0 + 16, HY1 - 10, 22):
    px = coast(py)
    box(px + 18, py, 0.2, 36, 4, 1.6, "stone")
    light(px + 34, py, 3, 500)
    for side in (-1, 1):
        for k in range(2):
            ship(px + 12 + k * 15, py + side * 6, R.uniform(11, 15), R.choice((0, math.pi)))
for _ in range(10):
    ship(R.uniform(300, 320), R.uniform(HY0 + 15, HY1 - 15), R.uniform(10, 16), R.uniform(0, math.tau), sailing=True)
for _ in range(22):
    sy = R.uniform(-480, 480)
    sx = R.uniform(coast(sy) + 20, 520)
    if not HY0 - 10 < sy < HY1 + 10 and math.hypot(sx - 262, sy - 150) > 40:
        ship(sx, sy, R.uniform(12, 22), R.uniform(-0.5, 0.5) + R.choice((0, math.pi)), sailing=True)
for _ in range(14):
    ship(R.uniform(345, 560), R.uniform(-200, 200), R.uniform(12, 22), R.uniform(-0.5, 0.5), sailing=True)
for wy in range(HY0 + 6, HY1, 19):
    wx = coast(wy) - 20
    box(wx, wy, 0, 12, 16, 6, "wall2")
    roof(wx, wy, 6, 13, 17, 2.6, "tile2")
    box(wx + 6.1, wy, 0, 0.12, 3, 3, "lamp" if R.random() < 0.6 else "dark")
reserve(225, (HY0 + HY1) / 2, 40, HY1 - HY0)
for _ in range(9):
    by = R.uniform(-220, 220)
    ship(river(by) + R.uniform(-4, 4), by, R.uniform(5, 7), math.pi / 2 + R.uniform(-0.2, 0.2))

# The aqueduct strides in from the hills to the north-west.
ax = -520
while ax < WX0 - 2:
    box(ax, 212, 0, 1.8, 2.4, 13, "stone")
    box(min(ax + 3.3, WX0 - 2 - 3.3), 212, 13, 6.6, 2.6, 1.6, "stone")
    ax += 6.6
box(WX0 + 9, 212, 0, 10, 10, 15, "stone")
roof(WX0 + 9, 212, 15, 11, 11, 3)
reserve(WX0 + 9, 212, 14, 14)

# ---- Walls: continuous runs, towers, gatehouses over the roads and a water gate over the river ----
def wall(a0, a1, at, horizontal, gates):
    """A wall from a0 to a1 along one axis, at `at` on the other. Every gap is a gate with an arch."""
    pos = lambda a, z=0: (a, at) if horizontal else (at, a)
    run = lambda a, b: box(*pos((a + b) / 2), 0, *((b - a, 3.4) if horizontal else (3.4, b - a)), 9, "stone")
    start = a0
    for c, half in sorted(gates):
        run(start, c - half)
        for side in (-1, 1):
            box(*pos(c + side * (half + 3)), 0, 8, 8, 15, "stone")
        box(*pos(c), 9, *((2 * half, 4.2) if horizontal else (4.2, 2 * half)), 4, "stone")
        light(*pos(c + half + 7), 6, 500)
        start = c + half
    run(start, a1)
    n = int((a1 - a0) // 30)
    for i in range(n + 1):
        a = a0 + (a1 - a0) * i / n
        if all(abs(a - c) > half + 8 for c, half in gates):
            box(*pos(a), 0, 7, 7, 13, "stone")


for wy in (WY0, WY1):
    wall(WX0, coast(wy) - 10, wy, True, [(0, 8), (river(wy), 14)])
wall(WY0, WY1, WX0, False, [(0, 8)])

# ---- The street grid, block by block ----
BW, BD, ST = 24, 18, 3
for gx in range(int(WX0) + 6, int(WX1) - BW, BW + ST):
    for gy in range(int(WY0) + 6, int(WY1) - BD, BD + ST):
        x0, y0, x1, y1 = gx, gy, gx + BW, gy + BD
        cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
        if abs(cx) < 16 or abs(cy) < 12 or not free(x0, y0, x1, y1) or x1 > coast(cy) - 6:
            continue  # the two main avenues stay open; landmarks, river and shore are left alone
        roll = R.random()
        if R.random() < 0.3:
            box(x0 - ST / 2, y0 - ST / 2, 0, 0.4, 0.4, 3, "lamp")
            light(x0 - ST / 2, y0 - ST / 2, 3.4, 250)
        if roll < 0.04:
            box(cx, cy, 0, BW, BD, 0.1, "grass")
            for _ in range(R.randint(3, 6)):
                tree(R.uniform(x0 + 2, x1 - 2), R.uniform(y0 + 2, y1 - 2), R.uniform(0.8, 1.2))
            continue
        cols, rows = R.choice((3, 3, 4)), R.choice((2, 3))
        for i in range(cols):
            for j in range(rows):
                lw, ld = BW / cols, BD / rows
                hx, hy = x0 + lw * (i + 0.5), y0 + ld * (j + 0.5)
                w, d = lw - R.uniform(0.6, 2.2), ld - R.uniform(0.6, 2.2)
                floors = R.choice((1, 2, 2, 3)) if roll > 0.12 else 4
                h = floors * 3.4
                box(hx, hy, 0, w, d, h, R.choice(("wall", "wall", "wall2")))
                for f in range(floors):
                    for k in range(max(1, int(w // 2.4))):
                        wx = hx - w / 2 + (k + 0.5) * w / max(1, int(w // 2.4))
                        box(wx, hy - d / 2 - 0.05, 1 + f * 3.4, 0.7, 0.12, 1.1, "lamp" if R.random() < 0.3 else "dark")
                    for k in range(max(1, int(d // 2.4))):
                        wy = hy - d / 2 + (k + 0.5) * d / max(1, int(d // 2.4))
                        box(hx + w / 2 + 0.05, wy, 1 + f * 3.4, 0.12, 0.7, 1.1, "lamp" if R.random() < 0.3 else "dark")
                roof(hx, hy, h, w + 0.8, d + 0.8, min(w, d) * 0.32, R.choice(("tile", "tile", "tile2")))

# Torches along the main avenues, cypresses beside them.
for t in range(int(WX0) + 10, int(WX1) - 10, 12):
    if abs(t - river(0)) > 16:
        for side in (-7, 7):
            box(t, side, 0, 0.5, 0.5, 2.6, "lamp")
            light(t, side, 3, 350)
for t in range(int(WY0) + 10, int(WY1) - 10, 12):
    if abs(t) > 18:
        for side in (-9, 9):
            cypress(side, t, 0.9)
        box(0, t, 0, 0.5, 0.5, 2.6, "lamp")
        light(0, t, 3, 350)
for (tx, ty) in ((WX0, y) for y in range(int(WY0), int(WY1), 30)):
    light(tx, ty, 15, 600)
for bx in (0, 120, -140):
    box(river(bx), bx, 0.1, 30, 7, 2.2, "stone")

# ---- Farmland: a patchwork of fields, vineyards, orchards and farmsteads ----
for x0, y0, x1, y1 in ((-1, WY0 - 400, 1, WY0), (-1, WY1, 1, WY1 + 400), (WX0 - 600, -1, WX0, 1)):
    box((x0 + x1) / 2, (y0 + y1) / 2, 0.02, max(x1 - x0, 7), max(y1 - y0, 7), 0.06, "dirt")
CW, CD = 46, 34
for fx in range(-700, 600, CW):
    for fy in range(-520, 520, CD):
        cx, cy = fx + CW / 2, fy + CD / 2
        near_city = WX0 - 16 < fx + CW and fx < coast(cy) + 4 and WY0 - 16 < fy + CD and fy < WY1 + 16
        if near_city or fx + CW > coast(cy) - 8 or abs(cx - river(cy)) < 14 + CW / 2 or abs(cx) < 5 + CW / 2 and abs(cy) > 200 \
                or abs(cy) < 5 + CD / 2 and cx < WX0 or 200 < cy < 224 and cx < WX0:
            continue  # the city, the shore, the river, the roads out of the gates and the aqueduct
        w, d, kind = CW - 2, CD - 2, R.choices(("wheat", "crop", "fallow", "vineyard", "orchard", "pasture"), (5, 4, 2, 2, 1, 2))[0]
        if kind in ("wheat", "crop"):
            colours = ("wheat", "wheat2") if kind == "wheat" else ("crop", "crop2")
            along_x = R.random() < 0.5
            for i in range(int((w if along_x else d) // 2)):
                o = -(w if along_x else d) / 2 + 1 + i * 2
                box(cx + o if along_x else cx, cy if along_x else cy + o, 0, *((2, d) if along_x else (w, 2)), 0.08, colours[i % 2])
        elif kind == "vineyard":
            box(cx, cy, 0, w, d, 0.07, "soil")
            for i in range(int(d // 2.6)):
                box(cx, cy - d / 2 + 1.3 + i * 2.6, 0, w - 2, 0.9, 1.1, "vine")
        elif kind == "orchard":
            box(cx, cy, 0, w, d, 0.07, "grass")
            for ox in range(int(w // 7)):
                for oy in range(int(d // 7)):
                    tree(cx - w / 2 + 3.5 + ox * 7, cy - d / 2 + 3.5 + oy * 7, 0.6)
        else:
            box(cx, cy, 0, w, d, 0.07, "soil" if kind == "fallow" else "field")
        if R.random() < 0.3:
            box(cx, cy + CD / 2 - 0.5, 0, CW, 1, 1.2, "vine")
        if R.random() < 0.06:
            box(cx + w / 4, cy, 0, 12, 8, 4, "wall")
            roof(cx + w / 4, cy, 4, 13, 9, 2.6)
            box(cx + w / 4 - 11, cy + 2, 0, 7, 10, 3.5, "wall2")
            roof(cx + w / 4 - 11, cy + 2, 3.5, 8, 11, 2.2, "tile2")
            box(cx + w / 4, cy - 4.05, 1, 0.8, 0.12, 1.1, "lamp")
            light(cx + w / 4, cy - 6, 2, 250)

# Countryside trees.
for _ in range(500):
    x, y = R.uniform(-700, 700), R.uniform(-500, 500)
    inside = WX0 - 8 < x < WX1 + 8 and WY0 - 8 < y < WY1 + 8
    if not inside and x < coast(y) - 8 and abs(x - river(y)) > 16:
        (cypress if R.random() < 0.3 else tree)(x, y, R.uniform(0.9, 1.4))

for mat, (verts, faces) in GEO.items():
    if verts:
        me = bpy.data.meshes.new(mat)
        me.from_pydata(verts, [], faces)
        me.materials.append(M[mat])
        scene.collection.objects.link(bpy.data.objects.new(mat, me))
for i, (x, y, z, watts) in enumerate(LIGHTS):
    lamp = bpy.data.lights.new(f"lamp{i}", "POINT")
    lamp.energy, lamp.color, lamp.shadow_soft_size = watts, (1.0, 0.62, 0.28), 0.5
    obj = bpy.data.objects.new(f"lamp{i}", lamp)
    obj.location = (x, y, z)
    scene.collection.objects.link(obj)

# ---- Sunset light, an orthographic 3/4 camera, full HD ----
world = bpy.data.worlds.new("dusk")
scene.world = world
world.node_tree.nodes["Background"].inputs["Color"].default_value = (0.12, 0.12, 0.26, 1)
world.node_tree.nodes["Background"].inputs["Strength"].default_value = 0.8
sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
sun.data.energy = 2.4
sun.data.angle = math.radians(1.5)
sun.data.color = (1.0, 0.48, 0.24)
sun.rotation_euler = (0, math.radians(-80), math.radians(28))
scene.collection.objects.link(sun)

cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
cam.data.type = "ORTHO"
cam.data.ortho_scale = 600
cam.data.clip_end = 3000
tilt, az = math.radians(57), math.radians(30)  # looking in from the south-east, low over the harbour
cam.rotation_euler = (tilt, 0, az)
cam.location = (75 + 1000 * math.sin(tilt) * math.sin(az), -35 - 1000 * math.sin(tilt) * math.cos(az), 1000 * math.cos(tilt))
scene.collection.objects.link(cam)
scene.camera = cam

scene.render.engine = "CYCLES"
prefs = bpy.context.preferences.addons["cycles"].preferences
for kind in ("OPTIX", "CUDA"):
    try:
        prefs.compute_device_type = kind
        prefs.get_devices()
        for dev in prefs.devices:
            dev.use = True
        scene.cycles.device = "GPU"
        break
    except TypeError:
        continue
scene.cycles.samples = 384
scene.cycles.use_denoising = True
scene.cycles.denoising_prefilter = "ACCURATE"
scene.view_settings.view_transform = "AgX"
scene.view_settings.look = "AgX - Punchy"
scene.render.resolution_x, scene.render.resolution_y = 1920, 1080
scene.render.use_stamp = False
scene.render.use_stamp_note = True
scene.render.stamp_note_text = "Rendered by app/scripts/city.py in Blender; no external art."
scene.render.image_settings.file_format = "WEBP"
scene.render.image_settings.quality = 92
scene.render.filepath = str(OUT)
bpy.ops.render.render(write_still=True)
