#!/usr/bin/env python3
"""Colour-vision-deficiency audit of the PyroSync design tokens.

Usage:
    python tools/cvd_check.py                  # Markdown report to stdout
    python tools/cvd_check.py --out PATH       # write the report to PATH
    python tools/cvd_check.py --self-test      # verify CIEDE2000 against Sharma et al. 2005

Requires Python 3 + numpy. Exit code 1 if any check on the CURRENT tokens FAILs.
"""
import argparse
import itertools
import math
import re
import sys
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parent.parent
TOKENS = ROOT / "web" / "src" / "styles" / "tokens.css"

# Machado, Oliveira & Fernandes (2009), severity 1.0, applied to linear RGB.
CVD_MATRICES = {
    "protan": np.array([[0.152286, 1.052583, -0.204868],
                        [0.114503, 0.786281, 0.099216],
                        [-0.003882, -0.048116, 1.051998]]),
    "deutan": np.array([[0.367322, 0.860646, -0.227968],
                        [0.280085, 0.672501, 0.047413],
                        [-0.011820, 0.042940, 0.968881]]),
    "tritan": np.array([[1.255528, -0.076749, -0.178779],
                        [-0.078411, 0.930809, 0.147602],
                        [0.004733, 0.691367, 0.303900]]),
}
CONDS = ["normal", "protan", "deutan", "tritan", "achromat"]
# Achromatopsia is a print/greyscale check; hue-coded sets cannot pass it by design.
COUNTED = CONDS[:4]
COND_TR = {"normal": "normal", "protan": "protanopi", "deutan": "deuteranopi",
           "tritan": "tritanopi", "achromat": "akromatopsi"}

# Sample points of the official matplotlib 'viridis' map at 8 evenly spaced positions
# (identical to R viridisLite::viridis(8)).
VIRIDIS8 = ["#440154", "#46327e", "#365c8d", "#277f8e",
            "#1fa187", "#4ac16d", "#9fda3a", "#fde725"]
# 'cividis' (Nunez, Anderton & Renslow 2018): 10 anchor values of matplotlib's cividis
# (R viridisLite option "E"); the 8 samples are linearly interpolated from these anchors.
CIVIDIS_ANCHORS = ["#00204d", "#00336f", "#39486b", "#575c6d", "#707173",
                   "#8a8779", "#a69d75", "#c4b46c", "#e3cc5c", "#ffea46"]

SENSOR_UI = ["--color-sensor-modis", "--color-sensor-viirs", "--color-duplicate-ring"]
SENSOR_MAP = ["--map-modis", "--map-viirs", "--map-duplicate-ring"]
STATUS = ["--color-status-live", "--color-status-demo", "--color-status-error"]
ACCENTS = ["--color-accent", "--color-accent-strong"]
FRP = [f"--frp-{i}" for i in range(1, 9)]
ANOM = ["--anom-neg-3", "--anom-neg-2", "--anom-neg-1", "--anom-0",
        "--anom-pos-1", "--anom-pos-2", "--anom-pos-3"]
SURFACES = ["--color-bg", "--color-surface", "--map-land", "--map-water"]
WANTED = (SENSOR_UI + SENSOR_MAP + STATUS + ACCENTS + FRP + ANOM + ["--anom-none"] + SURFACES)


def parse_tokens():
    css = TOKENS.read_text(encoding="utf-8")
    blocks = {
        "dark": re.search(r':root,\s*:root\[data-theme="dark"\]\s*\{(.*?)\n\}', css, re.S),
        "light": re.search(r':root\[data-theme="light"\]\s*\{(.*?)\n\}', css, re.S),
    }
    themes = {}
    for name, m in blocks.items():
        if not m:
            sys.exit(f"tokens.css: {name} theme block not found")
        vals = dict(re.findall(r"(--[\w-]+):\s*(#[0-9a-fA-F]{6})\s*;", m.group(1)))
        missing = [t for t in WANTED if t not in vals]
        if missing:
            sys.exit(f"tokens.css: {name} theme is missing {missing}")
        themes[name] = {k: v.lower() for k, v in vals.items()}
    return themes


# ---- colour maths -------------------------------------------------------------------

def hex_to_rgb(h):
    h = h.lstrip("#")
    return np.array([int(h[i:i + 2], 16) for i in (0, 2, 4)]) / 255.0


def rgb_to_hex(rgb):
    v = np.clip(np.round(np.asarray(rgb) * 255), 0, 255).astype(int)
    return "#{:02x}{:02x}{:02x}".format(*v)


def decode(c):
    c = np.asarray(c, dtype=float)
    return np.where(c <= 0.04045, c / 12.92, ((c + 0.055) / 1.055) ** 2.4)


def encode(c):
    c = np.clip(c, 0.0, 1.0)
    return np.where(c <= 0.0031308, c * 12.92, 1.055 * c ** (1 / 2.4) - 0.055)


def simulate(h, cond):
    if cond == "normal":
        return h
    lin = decode(hex_to_rgb(h))
    if cond == "achromat":
        y = 0.2126 * lin[0] + 0.7152 * lin[1] + 0.0722 * lin[2]
        lin = np.array([y, y, y])
    else:
        lin = np.clip(CVD_MATRICES[cond] @ lin, 0.0, 1.0)
    return rgb_to_hex(encode(lin))


_RGB2XYZ = np.array([[0.4124564, 0.3575761, 0.1804375],
                     [0.2126729, 0.7151522, 0.0721750],
                     [0.0193339, 0.1191920, 0.9503041]])
_WHITE = np.array([0.95047, 1.0, 1.08883])


def lab(h):
    xyz = (_RGB2XYZ @ decode(hex_to_rgb(h))) / _WHITE
    f = np.where(xyz > 216 / 24389, np.cbrt(xyz), (24389 / 27 * xyz + 16) / 116)
    return (116 * f[1] - 16, 500 * (f[0] - f[1]), 200 * (f[1] - f[2]))


def de00(l1, l2):
    L1, a1, b1 = l1
    L2, a2, b2 = l2
    c7 = 25.0 ** 7
    cbar = (math.hypot(a1, b1) + math.hypot(a2, b2)) / 2
    g = 0.5 * (1 - math.sqrt(cbar ** 7 / (cbar ** 7 + c7)))
    a1p, a2p = (1 + g) * a1, (1 + g) * a2
    c1p, c2p = math.hypot(a1p, b1), math.hypot(a2p, b2)
    h1p = math.degrees(math.atan2(b1, a1p)) % 360 if c1p else 0.0
    h2p = math.degrees(math.atan2(b2, a2p)) % 360 if c2p else 0.0
    dl, dc = L2 - L1, c2p - c1p
    if c1p * c2p == 0:
        dh = 0.0
    else:
        dh = h2p - h1p
        if dh > 180:
            dh -= 360
        elif dh < -180:
            dh += 360
    dH = 2 * math.sqrt(c1p * c2p) * math.sin(math.radians(dh / 2))
    lbp, cbp = (L1 + L2) / 2, (c1p + c2p) / 2
    if c1p * c2p == 0:
        hbp = h1p + h2p
    elif abs(h1p - h2p) <= 180:
        hbp = (h1p + h2p) / 2
    elif h1p + h2p < 360:
        hbp = (h1p + h2p + 360) / 2
    else:
        hbp = (h1p + h2p - 360) / 2
    t = (1 - 0.17 * math.cos(math.radians(hbp - 30)) + 0.24 * math.cos(math.radians(2 * hbp))
         + 0.32 * math.cos(math.radians(3 * hbp + 6)) - 0.20 * math.cos(math.radians(4 * hbp - 63)))
    dtheta = 30 * math.exp(-(((hbp - 275) / 25) ** 2))
    rc = 2 * math.sqrt(cbp ** 7 / (cbp ** 7 + c7))
    sl = 1 + 0.015 * (lbp - 50) ** 2 / math.sqrt(20 + (lbp - 50) ** 2)
    sc = 1 + 0.045 * cbp
    sh = 1 + 0.015 * cbp * t
    rt = -math.sin(math.radians(2 * dtheta)) * rc
    x, y = dc / sc, dH / sh
    return math.sqrt((dl / sl) ** 2 + x ** 2 + y ** 2 + rt * x * y)


def delta(h1, h2, cond="normal"):
    return de00(lab(simulate(h1, cond)), lab(simulate(h2, cond)))


def rel_lum(h):
    lin = decode(hex_to_rgb(h))
    return float(0.2126 * lin[0] + 0.7152 * lin[1] + 0.0722 * lin[2])


def contrast(h1, h2):
    a, b = sorted((rel_lum(h1), rel_lum(h2)), reverse=True)
    return (a + 0.05) / (b + 0.05)


def interpolate(anchors, n):
    pts = np.array([hex_to_rgb(a) for a in anchors])
    out = []
    for k in range(n):
        pos = k / (n - 1) * (len(anchors) - 1)
        i = min(int(pos), len(anchors) - 2)
        out.append(rgb_to_hex(pts[i] + (pts[i + 1] - pts[i]) * (pos - i)))
    return out


# ---- self test ----------------------------------------------------------------------

SHARMA = [  # (lab1, lab2, expected) from Sharma, Wu & Dalal 2005, Table 1
    ((50.0, 2.6772, -79.7751), (50.0, 0.0, -82.7485), 2.0425),
    ((50.0, 3.1571, -77.2803), (50.0, 0.0, -82.7485), 2.8615),
    ((50.0, 2.8361, -74.0200), (50.0, 0.0, -82.7485), 3.4412),
    ((50.0, 2.5, 0.0), (73.0, 25.0, -18.0), 27.1492),
    ((60.2574, -34.0099, 36.2677), (60.4626, -34.1751, 39.4387), 1.2644),
]


def self_test():
    ok = True
    for l1, l2, want in SHARMA:
        got = de00(l1, l2)
        good = abs(got - want) < 5e-4
        ok &= good
        print(f"{'ok  ' if good else 'FAIL'} dE00{l1} vs {l2}: {got:.4f} (expected {want})")
    w = lab("#ffffff")
    good = abs(w[0] - 100) < 0.01 and abs(w[1]) < 0.01 and abs(w[2]) < 0.01
    ok &= good
    print(f"{'ok  ' if good else 'FAIL'} white Lab = ({w[0]:.3f}, {w[1]:.3f}, {w[2]:.3f})")
    good = abs(contrast("#000000", "#ffffff") - 21) < 1e-9
    ok &= good
    print(f"{'ok  ' if good else 'FAIL'} WCAG black/white contrast = {contrast('#000000', '#ffffff'):.3f}")
    print("self-test:", "PASS" if ok else "FAIL")
    return ok


# ---- checks -------------------------------------------------------------------------

class Row:
    def __init__(self, theme, group, item, status, detail, counted=True):
        self.theme, self.group, self.item = theme, group, item
        self.status, self.detail, self.counted = status, detail, counted


def grade(v, ok, warn):
    return "PASS" if v >= ok else ("WARN" if v >= warn else "FAIL")


def worst(statuses):
    for s in ("FAIL", "WARN"):
        if s in statuses:
            return s
    return "PASS"


def short(t):
    return t.removeprefix("--")


def categorical(theme, tok, label, names, rows):
    for a, b in itertools.combinations(names, 2):
        vals = {c: delta(tok[a], tok[b], c) for c in CONDS}
        status = grade(min(vals[c] for c in COUNTED), 10, 6)
        detail = ", ".join(f"{COND_TR[c]} {vals[c]:.1f}" for c in CONDS)
        rows.append(Row(theme, label, f"{short(a)} / {short(b)}", status, "dE00: " + detail))


def sequential(theme, group, hexes, land, rows, counted=True, names=None):
    names = names or [f"{i + 1}" for i in range(len(hexes))]
    bad, dirs = [], {}
    for c in CONDS:
        ls = [lab(simulate(h, c))[0] for h in hexes]
        d = np.diff(ls)
        if np.all(d > 0):
            dirs[c] = "↑"
        elif np.all(d < 0):
            dirs[c] = "↓"
        else:
            bad.append(c)
    lines = ", ".join(f"{COND_TR[c]} {dirs.get(c, 'x')}" for c in CONDS)
    ls_norm = ", ".join(f"{lab(h)[0]:.0f}" for h in hexes)
    rows.append(Row(theme, group, "L* strictly monotonic", "FAIL" if bad else "PASS",
                    f"{lines}; normal L*: {ls_norm}", counted))
    per, wst = {}, (99.0, "", "")
    for c in CONDS:
        pairs = [(delta(hexes[i], hexes[i + 1], c), names[i], names[i + 1]) for i in range(len(hexes) - 1)]
        m = min(pairs)
        per[c] = m
        wst = min(wst, (m[0], f"{c} {m[1]}-{m[2]}", ""))
    detail = ", ".join(f"{COND_TR[c]} {per[c][0]:.1f} ({per[c][1]}-{per[c][2]})" for c in CONDS)
    rows.append(Row(theme, group, "min adjacent dE00 (>=5)", grade(wst[0], 5, 3), detail, counted))
    cs = [contrast(h, land) for h in hexes]
    low = [f"{names[i]}={cs[i]:.2f}" for i in range(len(hexes)) if cs[i] < 3]
    rows.append(Row(theme, group, "contrast vs map-land (>=3 desired)", grade(min(cs), 3, 1.5) if min(cs) < 3 else "PASS",
                    f"min {min(cs):.2f}; max {max(cs):.2f}; below 3:1: {', '.join(low) or 'none'}", counted))


def diverging(theme, tok, rows):
    g = "Sapma (anomali)"
    arms = {"neg": ["--anom-neg-1", "--anom-neg-2", "--anom-neg-3"],
            "pos": ["--anom-pos-1", "--anom-pos-2", "--anom-pos-3"]}
    for arm, names in arms.items():
        bad = []
        for c in CONDS:
            ls = [lab(simulate(tok[n], c))[0] for n in names]
            if not (all(np.diff(ls) > 0) or all(np.diff(ls) < 0)):
                bad.append(c)
        ls = ", ".join(f"{lab(tok[n])[0]:.0f}" for n in names)
        rows.append(Row(theme, g, f"{arm} arm L* monotonic (step 1->3)", "FAIL" if bad else "PASS",
                        f"normal L*: {ls}" + (f"; broken under {', '.join(bad)}" if bad else "")))
    for k in (1, 2, 3):
        a, b = f"--anom-neg-{k}", f"--anom-pos-{k}"
        vals = {c: delta(tok[a], tok[b], c) for c in CONDS}
        rows.append(Row(theme, g, f"neg-{k} vs pos-{k} (>=10)", grade(min(vals[c] for c in COUNTED), 10, 6),
                        "dE00: " + ", ".join(f"{COND_TR[c]} {vals[c]:.1f}" for c in CONDS)))
    vals = {c: delta(tok["--anom-0"], tok["--anom-none"], c) for c in CONDS}
    rows.append(Row(theme, g, "anom-0 vs anom-none (>=10)", grade(min(vals[c] for c in COUNTED), 10, 6),
                    "dE00: " + ", ".join(f"{COND_TR[c]} {vals[c]:.1f}" for c in CONDS)))
    c0 = lab(tok["--anom-0"])[0]
    l1 = {a: lab(tok[a])[0] for a in ("--anom-neg-1", "--anom-pos-1")}
    rows.append(Row(theme, g, "centre lightness note", "PASS",
                    f"anom-0 L* {c0:.0f}; neg-1 {l1['--anom-neg-1']:.0f}, pos-1 {l1['--anom-pos-1']:.0f} "
                    "(centre is the 'quiet' end against the map; arms are checked from step 1)"))


def ui_contrast(theme, tok, rows):
    for n in ACCENTS + STATUS:
        cb, cs = contrast(tok[n], tok["--color-bg"]), contrast(tok[n], tok["--color-surface"])
        rows.append(Row(theme, "Metin/UI kontrastı", short(n), "PASS" if min(cb, cs) >= 3 else "FAIL",
                        f"vs color-bg {cb:.2f}, vs color-surface {cs:.2f} (>=3:1)"))


def alternatives(theme, tok, rows, verdicts):
    land = tok["--map-land"]
    for name, hexes in (("viridis", VIRIDIS8), ("cividis", interpolate(CIVIDIS_ANCHORS, 8))):
        sub = []
        sequential(theme, f"Aday: {name}", hexes, land, sub, counted=False)
        rows.extend(sub)
        c_low, c_high = contrast(hexes[0], land), contrast(hexes[-1], land)
        dark_to_light = c_high >= c_low
        fine = sub[0].status == "PASS" and sub[1].status != "FAIL" and max(c_low, c_high) >= 3
        direction = "koyudan açığa (1=koyu, 8=açık)" if dark_to_light else "açıktan koyuya (1=açık, 8=koyu)"
        verdicts.append((theme, name, "uygun" if fine else "uygun değil", direction, c_low, c_high, sub))


# ---- report -------------------------------------------------------------------------

def md_table(head, body):
    out = ["| " + " | ".join(head) + " |", "|" + "|".join("---" for _ in head) + "|"]
    out += ["| " + " | ".join(str(c) for c in r) + " |" for r in body]
    return out


def ramp_table(tok, names):
    body = []
    for n in names:
        body.append([f"`{short(n)}`"] + [f"`{simulate(tok[n], c)}`" for c in CONDS])
    return md_table(["Token"] + [COND_TR[c] for c in CONDS], body)


def render(themes):
    rows, verdicts = [], []
    for theme, tok in themes.items():
        categorical(theme, tok, "Kategorik: sensör UI", SENSOR_UI, rows)
        categorical(theme, tok, "Kategorik: harita sensörleri", SENSOR_MAP, rows)
        categorical(theme, tok, "Kategorik: durum", STATUS, rows)
        sequential(theme, "Sıralı FRP", [tok[n] for n in FRP], tok["--map-land"], rows,
                   names=[str(i) for i in range(1, 9)])
        diverging(theme, tok, rows)
        ui_contrast(theme, tok, rows)
        alternatives(theme, tok, rows, verdicts)

    counted = [r for r in rows if r.counted]
    fails = [r for r in counted if r.status == "FAIL"]
    warns = [r for r in counted if r.status == "WARN"]
    out = [
        "# Renk Denetimi (C7)",
        "",
        "Bu rapor `web/src/styles/tokens.css` içindeki renklerin renk körlüğü altında ayırt "
        "edilebilirliğini denetler: sensör renkleri (UI + harita), durum renkleri, FRP sıralı skalası, "
        "anomali sapma skalası, vurgu/durum renklerinin zemin kontrastı. Koyu ve açık tema ayrı ayrı "
        "değerlendirilir.",
        "",
        "**Yeniden üretme:** `python tools/cvd_check.py --out docs/RENK_DENETIMI.md` "
        "(Python 3 + numpy; `--self-test` CIEDE2000 gerçeklemesini doğrular; mevcut token'larda FAIL varsa "
        "çıkış kodu 1). Bu dosya elle düzenlenmez.",
        "",
        "**Yöntem:** Protanopi, deuteranopi ve tritanopi, Machado, Oliveira & Fernandes (2009) matrisleriyle "
        "(şiddet 1.0) doğrusal RGB üzerinde simüle edilir (sRGB gama çözme/kodlama dahil). Akromatopsi, "
        "yalnızca parlaklığa indirgenmiş gri tondur (baskı/gri kontrol). Renk farkı CIEDE2000 (ΔE00), "
        "kontrast WCAG 2.x göreli parlaklık oranıdır. Eşikler: kategorik ΔE00 >= 10 (WARN 6-10), "
        "sıralı komşu adım ΔE00 >= 5 (WARN 3-5), grafik/UI kontrastı >= 3:1. "
        "Akromatopsi kategorik ve sapma çiftlerinde yalnızca bilgi amaçlıdır (bu setler ton ile "
        "kodlanır; durum rengi her zaman metinle, sensör ise şekille (MODIS içi boş daire, VIIRS dolu kare, "
        "`docs/TASARIM_SISTEMI.md`) eşlenir); sıralı skalada her durumda sayılır.",
        "",
        f"**Özet (mevcut token'lar):** {len(fails)} FAIL, {len(warns)} WARN, "
        f"{len(counted) - len(fails) - len(warns)} PASS. Aday paletler (viridis/cividis) sayıma girmez.",
        "",
    ]
    if fails or warns:
        out += md_table(["Tema", "Grup", "Öğe", "Durum", "Ayrıntı"],
                        [[r.theme, r.group, r.item, f"**{r.status}**", r.detail] for r in fails + warns])
        out.append("")

    for theme, tok in themes.items():
        out += [f"## Tema: {theme}", ""]
        out += [f"Zemin: color-bg `{tok['--color-bg']}`, color-surface `{tok['--color-surface']}`, "
                f"map-land `{tok['--map-land']}`, map-water `{tok['--map-water']}`.", ""]
        out += ["### FRP skalası (normal ve simüle hex)", ""] + ramp_table(tok, FRP) + [""]
        out += ["### Anomali skalası (normal ve simüle hex)", ""]
        out += ramp_table(tok, ANOM + ["--anom-none"]) + [""]
        groups = list(dict.fromkeys(r.group for r in rows if r.theme == theme and r.counted))
        for g in groups:
            out += [f"### {g}", ""]
            out += md_table(["Öğe", "Durum", "Ayrıntı"],
                            [[r.item, r.status, r.detail] for r in rows if r.theme == theme and r.group == g])
            out.append("")

    out += ["## Aday sıralı paletler (K4 renk körlüğü dostu seçenek)", "",
            "Kaynak: viridis, matplotlib 'viridis' haritasının 8 eşit aralıklı örneği; cividis, Nuñez, "
            "Anderton & Renslow (2018), matplotlib 'cividis' 10 çıpa değerinden 8 örneğe doğrusal "
            "interpolasyonla türetilmiştir.", ""]
    for name, hexes in (("viridis", VIRIDIS8), ("cividis", interpolate(CIVIDIS_ANCHORS, 8))):
        out += [f"- {name}: " + " ".join(f"`{h}`" for h in hexes)]
    out.append("")
    out += md_table(["Tema", "Palet", "Karar", "Önerilen yön (düşük → yüksek)", "Kontrast: doğal 1. adım (koyu uç)", "Kontrast: doğal 8. adım (açık uç)"],
                    [[t, n, v, d, f"{cl:.2f}", f"{ch:.2f}"] for t, n, v, d, cl, ch, _ in verdicts])
    out += ["", "Karar: L* tüm simülasyonlarda monoton, komşu ΔE00 FAIL değil ve sıcak uç map-land'a karşı >= 3:1 ise "
            "'uygun'. Düşük uç zeminle kaynaşabilir (düşük FRP için kabul edilebilir); ayrıntılar aşağıda.", ""]
    for theme in themes:
        for name in ("viridis", "cividis"):
            g = f"Aday: {name}"
            out += [f"### {theme} / {name}", ""]
            out += md_table(["Öğe", "Durum", "Ayrıntı"],
                            [[r.item, r.status, r.detail] for r in rows if r.theme == theme and r.group == g])
            out.append("")
    return "\n".join(out), bool(fails)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", help="write the Markdown report to this path")
    ap.add_argument("--self-test", action="store_true", help="verify CIEDE2000 against Sharma et al. 2005")
    args = ap.parse_args()
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8")
    if args.self_test:
        sys.exit(0 if self_test() else 1)
    report, failed = render(parse_tokens())
    if args.out:
        Path(args.out).write_text(report + "\n", encoding="utf-8")
        print(f"wrote {args.out}")
    else:
        print(report)
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
