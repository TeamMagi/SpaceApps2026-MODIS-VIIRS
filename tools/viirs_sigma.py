"""Olcum: VIIRS tek-piksel goreli FRP sigma'si (S9).

Es zamanli (|dt| <= 5 dk) farkli-uydu VIIRS ciftlerinden (SNPP x NOAA-20), FRP > 40 MW ve
A_p = scan*track < 0.3 km2 kosuluyla goreli fark d = (a-b)/mean(a,b) hesaplanir.
sigma = 1.4826*MAD(d) / sqrt(2)  (iki esit sensorun farki icin).

Kullanim (yalniz stdlib):
  python -I tools/viirs_sigma.py [ARSIV_DIZINI] [YIL,YIL,...] [ULKE,ULKE,...]
Arsiv dizini: ilk arguman, yoksa ARCHIVE_CACHE_DIR ortam degiskeni, yoksa .cache/firms-archive.
Varsayilan: 2018-2024, buyuk yangin ulkeleri.
"""
import csv, math, os, statistics, sys
from collections import defaultdict
from pathlib import Path

root = Path(sys.argv[1] if len(sys.argv) > 1 else os.environ.get("ARCHIVE_CACHE_DIR", ".cache/firms-archive"))
years = (sys.argv[2] if len(sys.argv) > 2 else "2018,2019,2020,2021,2022,2023,2024").split(",")
countries = (sys.argv[3] if len(sys.argv) > 3 else
             "Russian_Federation,Canada,Brazil,Australia,Angola,United_States_of_America,Bolivia").split(",")
MAX_DT_MIN, MIN_FRP, MAX_AREA, MAX_DIST_M = 5, 40.0, 0.3, 375.0


def load(slug, year, country):
    p = root / slug / year / f"{slug}_{year}_{country}.csv"
    if not p.exists():
        return []
    out = []
    with p.open(newline="", encoding="utf-8") as f:
        for r in csv.DictReader(f):
            try:
                frp = float(r["frp"]); area = float(r["scan"]) * float(r["track"])
                if frp <= MIN_FRP or area >= MAX_AREA:
                    continue
                t = r["acq_time"].zfill(4)
                m = int(t[:2]) * 60 + int(t[2:])
                out.append((r["acq_date"], m, float(r["latitude"]), float(r["longitude"]), frp))
            except (ValueError, KeyError):
                continue
    return out


def dist_m(la1, lo1, la2, lo2):
    dx = math.radians(lo2 - lo1) * math.cos(math.radians((la1 + la2) / 2))
    dy = math.radians(la2 - la1)
    return 6371000.0 * math.hypot(dx, dy)


diffs, n_cand = [], 0
for y in years:
    for c in countries:
        a, b = load("viirs-snpp", y, c), load("viirs-jpss1", y, c)
        if not a or not b:
            continue
        grid = defaultdict(list)
        for r in b:
            grid[(r[0], round(r[2] / 0.01), round(r[3] / 0.01))].append(r)
        for d, m, la, lo, frp in a:
            for i in (-1, 0, 1):
                for j in (-1, 0, 1):
                    for r in grid.get((d, round(la / 0.01) + i, round(lo / 0.01) + j), ()):
                        if abs(r[1] - m) <= MAX_DT_MIN and dist_m(la, lo, r[2], r[3]) <= MAX_DIST_M:
                            n_cand += 1
                            diffs.append((frp - r[4]) / ((frp + r[4]) / 2))

n = len(diffs)
print(f"pair_count={n}")
if n < 2:
    print("sigma: olculemedi (cift yok)")
    sys.exit(0)
med = statistics.median(diffs)
mad = statistics.median(abs(x - med) for x in diffs)
sigma = 1.4826 * mad / math.sqrt(2)
sd = statistics.stdev(diffs) / math.sqrt(2)
se = sigma / math.sqrt(2 * (n - 1))
print(f"median_rel_diff={med:+.4f}  robust_sigma={sigma:.4f}  classical_sigma={sd:.4f}  approx_se={se:.4f}")
print("guven: " + ("saglam (>=300 cift)" if n >= 300 else "YETERSIZ (<300 cift); VIIRS sabitini degistirme"))
