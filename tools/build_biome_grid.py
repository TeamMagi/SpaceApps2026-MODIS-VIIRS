#!/usr/bin/env python3
"""Build the 0.25 degree biome grid (data/landcover/biome_0p25.rle) for PyroSync (K14).

Source: WRF/WPS static geography set `modis_landuse_20class_5m_with_lakes`
(MODIS IGBP land cover, 20 classes + lakes, 5 arc-minute, 4320 x 2160).

Usage:
  python3 tools/build_biome_grid.py --download                  # fetch the tarball, then build
  python3 tools/build_biome_grid.py --source lc5m.tar.bz2       # build from a tarball
  python3 tools/build_biome_grid.py --source extracted_dir/     # build from an extracted dir
Options: --out-dir (default data/landcover), --download-date YYYY-MM-DD (only written to SOURCE.md).

Python 3 stdlib only; numpy is used when importable (same result, much faster).
The archive is treated as untrusted data: tar members are read in memory by exact
name and nothing is extracted to disk or executed.

Output format (little endian):
  8 bytes  magic "PSBIOME1"
  u16      width  (1440, col 0 = 180 deg W)
  u16      height (720, row 0 = northernmost)
  then repeated (u16 run length, u8 biome code) pairs in row-major order;
  runs may cross row ends and are at most 65535 long.
"""

import argparse
import datetime
import hashlib
import io
import os
import re
import struct
import sys
import tarfile
import urllib.request

URL = "https://www2.mmm.ucar.edu/wrf/src/wps_files/modis_landuse_20class_5m_with_lakes.tar.bz2"
TOP = "modis_landuse_20class_5m_with_lakes"
MAGIC = b"PSBIOME1"
W, H = 1440, 720
SRC_W, SRC_H = 4320, 2160
FACTOR = 3

NON_BURNABLE, FOREST, SHRUBLAND, SAVANNA, GRASSLAND, CROPLAND, WETLAND, TUNDRA = range(8)
NAMES = ["NonBurnable", "Forest", "Shrubland", "Savanna", "Grassland", "Cropland", "Wetland", "Tundra"]

# IGBP-MODIS class number (1-based layer) -> biome group; verified against LANDUSE.TBL
# MODIFIED_IGBP_MODIS_NOAH (21 categories).
CLASS_TO_BIOME = {
    1: FOREST, 2: FOREST, 3: FOREST, 4: FOREST, 5: FOREST,
    6: SHRUBLAND, 7: SHRUBLAND,
    8: SAVANNA, 9: SAVANNA,
    10: GRASSLAND,
    11: WETLAND,
    12: CROPLAND,
    13: NON_BURNABLE,
    14: CROPLAND,
    15: NON_BURNABLE,
    16: NON_BURNABLE,
    17: NON_BURNABLE,
    18: TUNDRA, 19: TUNDRA, 20: TUNDRA,
    21: NON_BURNABLE,
}

TILE_RE = re.compile(r"^(\d{5})-(\d{5})\.(\d{5})-(\d{5})$")


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def parse_index(text):
    meta = {}
    for line in text.splitlines():
        if "=" in line:
            k, v = line.split("=", 1)
            meta[k.strip()] = v.strip().strip('"')
    return meta


def load_source(path):
    """Return (index_text, {tile_name: bytes}, tarball_sha256_or_None)."""
    tiles = {}
    index_text = None
    tar_sha = None
    if os.path.isdir(path):
        base = os.path.join(path, TOP) if os.path.isdir(os.path.join(path, TOP)) else path
        for name in sorted(os.listdir(base)):
            full = os.path.join(base, name)
            if name == "index":
                index_text = open(full, encoding="utf-8").read()
            elif TILE_RE.match(name):
                tiles[name] = open(full, "rb").read()
    else:
        raw = open(path, "rb").read()
        tar_sha = sha256(raw)
        with tarfile.open(fileobj=io.BytesIO(raw), mode="r:*") as tf:
            for m in tf.getmembers():
                if not m.isfile():
                    continue
                name = m.name.split("/")[-1]
                if m.name != f"{TOP}/{name}":
                    continue
                if name == "index":
                    index_text = tf.extractfile(m).read().decode("utf-8")
                elif TILE_RE.match(name):
                    tiles[name] = tf.extractfile(m).read()
    if index_text is None:
        sys.exit("error: index file not found in source")
    return index_text, tiles, tar_sha


def check_index(meta):
    expect = {
        "type": "continuous", "category_min": "1", "category_max": "21", "projection": "regular_ll",
        "wordsize": "1", "tile_x": "600", "tile_y": "600", "tile_z": "21", "tile_bdr": "3",
        "mminlu": "MODIFIED_IGBP_MODIS_NOAH", "iswater": "17", "islake": "21", "isice": "15", "isurban": "13",
    }
    for k, v in expect.items():
        if meta.get(k) != v:
            sys.exit(f"error: unexpected index value {k}={meta.get(k)!r} (expected {v!r})")
    if abs(float(meta["dx"]) - 1 / 12) > 1e-4 or abs(float(meta["dy"]) - 1 / 12) > 1e-4:
        sys.exit("error: unexpected resolution in index")
    if abs(float(meta["known_lat"]) + 89.9583) > 1e-3 or abs(float(meta["known_lon"]) + 179.9583) > 1e-3:
        sys.exit("error: unexpected origin in index")


def dominant_classes(tile, np):
    """Per-cell dominant IGBP class (1..21) of one tile's interior, rows south to north.

    Layers are category fractions in percent; ties go to the lowest class number.
    """
    bdr, t = 3, 600
    side = t + 2 * bdr
    plane = side * side
    if len(tile) != plane * 21:
        sys.exit("error: unexpected tile size")
    if np is not None:
        a = np.frombuffer(tile, dtype=np.uint8).reshape(21, side, side)
        a = a[:, bdr:bdr + t, bdr:bdr + t]
        return (a.argmax(axis=0) + 1).astype(np.uint8)
    rows = []
    for y in range(bdr, bdr + t):
        off = y * side + bdr
        best = list(tile[off:off + t])
        cls = [1] * t
        for z in range(1, 21):
            cur = tile[z * plane + off:z * plane + off + t]
            for i in range(t):
                if cur[i] > best[i]:
                    best[i] = cur[i]
                    cls[i] = z + 1
        rows.append(cls)
    return rows


def build_source_grid(tiles, np):
    """Dominant class grid of 4320 x 2160, rows south to north (as in the source)."""
    # 4320 x 2160 is not a multiple of 600: the last tile column/row is padded and cropped here.
    n_expected = -(-SRC_W // 600) * -(-SRC_H // 600)
    if len(tiles) != n_expected:
        sys.exit(f"error: expected {n_expected} tiles, found {len(tiles)}")
    if np is not None:
        grid = np.zeros((2400, 4800), dtype=np.uint8)
    else:
        grid = [bytearray(4800) for _ in range(2400)]
    for name in sorted(tiles):
        x0, x1, y0, y1 = (int(g) for g in TILE_RE.match(name).groups())
        if (x1 - x0, y1 - y0) != (599, 599) or (x0 - 1) % 600 or (y0 - 1) % 600 or x1 > 4800 or y1 > 2400:
            sys.exit(f"error: unexpected tile range {name}")
        part = dominant_classes(tiles[name], np)
        if np is not None:
            grid[y0 - 1:y1, x0 - 1:x1] = part
        else:
            for j, row in enumerate(part):
                grid[y0 - 1 + j][x0 - 1:x1] = bytes(row)
    if np is not None:
        return grid[:SRC_H, :SRC_W]
    return [row[:SRC_W] for row in grid[:SRC_H]]


def aggregate(src, np):
    """3 x 3 majority vote; burnable classes win over non-burnable ones."""
    lut = [0] * 22
    for c, b in CLASS_TO_BIOME.items():
        lut[c] = b
    if np is not None:
        biome = np.array(lut, dtype=np.uint8)[src][::-1]  # north first
        blocks = biome.reshape(H, FACTOR, W, FACTOR).transpose(0, 2, 1, 3).reshape(H, W, FACTOR * FACTOR)
        counts = np.stack([(blocks == b).sum(axis=2) for b in range(1, 8)], axis=2)  # codes 1..7
        winner = counts.argmax(axis=2).astype(np.uint8) + 1  # ties to the lowest code
        out = np.where(counts.sum(axis=2) > 0, winner, 0).astype(np.uint8)
        return bytes(out.tobytes())
    out = bytearray(W * H)
    for r in range(H):
        rows = [bytes(lut[c] for c in src[SRC_H - 1 - (FACTOR * r + k)]) for k in range(FACTOR)]
        for c in range(W):
            counts = [0] * 8
            for row in rows:
                for b in row[FACTOR * c:FACTOR * c + FACTOR]:
                    counts[b] += 1
            best, best_n = 0, 0
            for b in range(1, 8):
                if counts[b] > best_n:
                    best, best_n = b, counts[b]
            out[r * W + c] = best
    return bytes(out)


def rle_encode(cells):
    out = bytearray(MAGIC + struct.pack("<HH", W, H))
    i, n = 0, len(cells)
    while i < n:
        v = cells[i]
        j = i
        while j < n and cells[j] == v and j - i < 65535:
            j += 1
        out += struct.pack("<HB", j - i, v)
        i = j
    return bytes(out)


def rle_decode(data):
    assert data[:8] == MAGIC
    w, h = struct.unpack_from("<HH", data, 8)
    cells = bytearray()
    for off in range(12, len(data), 3):
        run, v = struct.unpack_from("<HB", data, off)
        cells += bytes([v]) * run
    assert (w, h) == (W, H) and len(cells) == W * H
    return bytes(cells)


def shares(cells):
    counts = [0] * 8
    for v in cells:
        counts[v] += 1
    land = sum(counts) - counts[0]
    return counts, land


SOURCE_MD = """# Arazi Örtüsü / Biyom Izgarası — Kaynak ve Lisans Notu

`biome_0p25.rle`, K14 (biyom bazlı tamlık katsayısı) için küresel 0.25° biyom katmanıdır. Dosya
`crates/core/src/biome.rs` içine `include_bytes!` ile gömülür; çalışma anında dosya yolu, ayar veya I/O yoktur.

## Kaynak
- **Veri seti:** WRF/WPS statik coğrafya verisi `modis_landuse_20class_5m_with_lakes` (NCAR/MMM).
- **URL:** <{url}>
- **İndirme tarihi:** {date} (oturum açmadan indirilebilir; tar.bz2, {tar_size} bayt)
- **Tarball SHA-256:** `{tar_sha}`
- **Çıktı SHA-256:** `{out_sha}` (`biome_0p25.rle`, {out_size} bayt)
- **İçerik:** MODIS IGBP arazi örtüsü, 20 sınıf + göl sınıfı (Noah-modified 21 kategori, `mminlu="MODIFIED_IGBP_MODIS_NOAH"`),
  5 yay dakikası (4320 × 2160, `dx = dy = 0.0833333`), geogrid ikili döşemeleri (600 × 600, 3 hücre sınır, 21 katman,
  `wordsize = 1`). Her katman bir kategorinin hücredeki yüzde payıdır; satır sırası güneyden kuzeye (`known_lat = -89.9583`).
- **`index` dosyasındaki açıklama (birebir):**
  `description="{description}"`
- **Lisans:** Kaynak sayfada açık bir lisans metni doğrulanmadı; veri halka açık NCAR/MMM WPS dağıtımıdır. Dağıtım
  öncesi kaynağın kullanım koşulları ayrıca teyit edilmelidir.
- **Türetildiği MODIS koleksiyonu/yılları:** `index` ve dosya adı bunu belirtmez; yalnızca "30 yay saniyesinden
  NCAR/MMM tarafından 2018-05-23'te alt örneklendi" bilgisi vardır. Koleksiyon ve yıl bu veriden doğrulanamaz.

## Neden bu kaynak (ham MCD12Q1 değil)
MCD12Q1, Earthdata girişi gerektiren 500 m sinüzoidal HDF döşemeleridir (yıl başına yüzlerce döşeme). K14 piksel
değil, takvim hücresi/bölge başına tek bir biyom katmanına ihtiyaç duyar; bu veri seti 8 MB ve girişsiz iner.

## Sınıf eşlemesi (IGBP → biyom grubu)
| Kod | Biyom | IGBP sınıfları |
|---|---|---|
| 0 | NonBurnable | 13 kentsel, 15 kar/buz, 16 çıplak, 17 su, 21 göl |
| 1 | Forest | 1–5 (yaprak dökmez/döken iğne/geniş yapraklı, karışık orman) |
| 2 | Shrubland | 6–7 (kapalı/açık çalılık) |
| 3 | Savanna | 8 odunsu savan, 9 savan |
| 4 | Grassland | 10 otlak |
| 5 | Cropland | 12 tarım, 14 tarım/doğal mozaik |
| 6 | Wetland | 11 kalıcı sulak alan |
| 7 | Tundra | 18 ağaçlı tundra, 19 karışık tundra, 20 çıplak tundra |

Sınıf numaraları `index` dosyasıyla (`category_max=21`, `iswater=17`, `islake=21`, `isice=15`, `isurban=13`) ve WPS
`LANDUSE.TBL` `MODIFIED_IGBP_MODIS_NOAH` tablosuyla uyumludur.

## Toplulaştırma kuralı
1. Her 5' hücrede en büyük yüzdeli sınıf alınır (eşitlikte en küçük sınıf numarası).
2. 0.25° hücre = 3 × 3 kaynak hücre. Yanabilir (kod 1–7) bir grup varsa çoğunluk yalnızca yanabilir gruplar
   arasından seçilir (kıyı/göl hücreleri suya kaybedilmez); eşitlikte küçük kod kazanır.
3. Hücre yalnızca 9 kaynak hücrenin tamamı yanamaz ise NonBurnable (0) olur.
4. Izgara 1440 × 720, satır 0 = en kuzey, sütun 0 = 180°B. Çıktı belirlenimcidir (aynı girdi → aynı bayt).

## Dosya biçimi (`biome_0p25.rle`)
Little endian: 8 bayt sihirli sayı `PSBIOME1`, `u16` genişlik, `u16` yükseklik, ardından satır-öncelikli sırada
(`u16` tekrar sayısı, `u8` sınıf) çiftleri. Tekrarlar satır sonunda durmak zorunda değildir.

## Yeniden üretim
```
python3 tools/build_biome_grid.py --download
# veya, tarball elde varsa:
python3 tools/build_biome_grid.py --source modis_landuse_20class_5m_with_lakes.tar.bz2
```

## Sınırlamalar
- Statik iklimlemedir, yıllık MCD12Q1 değildir: ormansızlaşma gibi arazi örtüsü değişimi izlenmez.
- 0.25° çoğunluk oyu küçük parçaları (dar sulak alan, küçük tarım parseli, kentler) gizler.
- Yanabilir sınıf önceliği kıyı ve göl hücrelerini kasıtlı olarak kara lehine yanlı kılar.
- Kaynağın MODIS koleksiyonu/yılı belirtilmemiştir (yukarıya bakın).
"""


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--source", help="tar.bz2 tarball or extracted directory")
    ap.add_argument("--download", action="store_true", help="download the tarball first")
    ap.add_argument("--out-dir", default=os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "data", "landcover"))
    ap.add_argument("--download-date", default=datetime.date.today().isoformat())
    args = ap.parse_args()

    source = args.source
    if args.download:
        source = source or "modis_landuse_20class_5m_with_lakes.tar.bz2"
        print(f"downloading {URL}")
        urllib.request.urlretrieve(URL, source)
    if not source:
        ap.error("give --source or --download")

    try:
        import numpy as np
    except ImportError:
        np = None

    index_text, tiles, tar_sha = load_source(source)
    meta = parse_index(index_text)
    check_index(meta)
    src = build_source_grid(tiles, np)
    cells = aggregate(src, np)
    encoded = rle_encode(cells)
    assert rle_decode(encoded) == cells

    out_dir = os.path.abspath(args.out_dir)
    os.makedirs(out_dir, exist_ok=True)
    with open(os.path.join(out_dir, "biome_0p25.rle"), "wb") as f:
        f.write(encoded)
    out_sha = sha256(encoded)
    tar_size = os.path.getsize(source) if tar_sha else "-"
    md = SOURCE_MD.format(
        url=URL, date=args.download_date, tar_size=tar_size, tar_sha=tar_sha or "(dizinden okundu, hesaplanmadı)",
        out_sha=out_sha, out_size=len(encoded), description=meta["description"],
    )
    with open(os.path.join(out_dir, "SOURCE.md"), "w", encoding="utf-8", newline="\n") as f:
        f.write(md)

    print(f"raw {len(cells)} bytes, encoded {len(encoded)} bytes ({len(encoded) / len(cells):.1%})")
    print(f"output sha256 {out_sha}")
    counts, land = shares(cells)
    for b in range(8):
        share = "" if b == 0 else f"  {counts[b] / land:.1%} of land"
        print(f"  {b} {NAMES[b]:<12}{counts[b]:>8}{share}")


if __name__ == "__main__":
    main()
