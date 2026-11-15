#!/usr/bin/env python3
"""Flatten ESA Fire_cci v5.1 (FireCCI51) 0.25 deg monthly grid files into one CSV for Y10.

Reads the yearly CEDA zips (Compressed/<year>.zip) without unpacking them to disk and writes
every cell-month with burned_area > 0 as `year,month,lat,lon,burned_area_m2,observed_fraction`
(lat/lon = cell centre). The Rust example `burned_area_validation` reads this CSV.

Usage: python -I tools/firecci_extract.py <zip-dir> <out.csv>
  e.g. python -I tools/firecci_extract.py .cache/firecci/zip .cache/firecci/firecci51_nonzero.csv
Download: https://dap.ceda.ac.uk/neodc/esacci/fire/data/burned_area/MODIS/grid/v5.1/Compressed/
Requires Python 3 + numpy + scipy (scipy.io.netcdf_file; the files are NetCDF-3 classic).
"""

import io
import re
import sys
import zipfile
from pathlib import Path

import numpy as np
from scipy.io import netcdf_file

NAME = re.compile(r"(\d{4})(\d{2})01-ESACCI-L4_FIRE-BA-MODIS-fv5\.1\.nc$")


def members(zf: zipfile.ZipFile) -> dict:
    """(year, month) -> member name; a `new-corrected` copy wins over any other."""
    out = {}
    for name in zf.namelist():
        m = NAME.search(name)
        if not m or "redacted" in name:
            continue
        key = (int(m.group(1)), int(m.group(2)))
        if key not in out or "new-corrected" in name:
            out[key] = name
    return out


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__, file=sys.stderr)
        return 2
    zip_dir, out_path = Path(sys.argv[1]), Path(sys.argv[2])
    zips = sorted(p for p in zip_dir.glob("*.zip") if re.fullmatch(r"\d{4}\.zip", p.name))
    if not zips:
        print(f"no <year>.zip files in {zip_dir}", file=sys.stderr)
        return 1
    rows = 0
    with out_path.open("w", newline="") as out:
        out.write("year,month,lat,lon,burned_area_m2,observed_fraction\n")
        for zpath in zips:
            with zipfile.ZipFile(zpath) as zf:
                for (year, month), name in sorted(members(zf).items()):
                    with netcdf_file(io.BytesIO(zf.read(name)), "r", mmap=False) as nc:
                        lat = np.asarray(nc.variables["lat"][:], dtype=float)
                        lon = np.asarray(nc.variables["lon"][:], dtype=float)
                        ba = np.asarray(nc.variables["burned_area"][0], dtype=float)
                        obs = np.asarray(nc.variables["fraction_of_observed_area"][0], dtype=float)
                    iy, ix = np.nonzero(ba > 0)
                    for y, x in zip(iy, ix):
                        out.write(f"{year},{month},{lat[y]:.3f},{lon[x]:.3f},{ba[y, x]:.0f},{obs[y, x]:.4f}\n")
                    rows += len(iy)
                    print(f"{year}-{month:02d}: {len(iy)} burned cells", file=sys.stderr)
    print(f"wrote {rows} rows to {out_path}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
