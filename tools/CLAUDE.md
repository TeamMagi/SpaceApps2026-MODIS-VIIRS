# tools — Yardımcı Scriptler

## Kesin Kurallar
- Commit mesajlarına `Co-Authored-By` veya AI imzası eklenmez.
- Scriptler POSIX sh/bash uyumlu yazılır.
- Her script `set -euo pipefail` ile başlar.

## Mevcut Scriptler

| Script | Görev |
|--------|-------|
| `check-env.sh` | Geliştirme ortamı kontrol (cargo, node, npm, docker, .env) |
| `fetch-sample.sh` | FIRMS API'den örnek CSV verisi indirme |
| `build-globe-tiles.sh` | NASA Blue Marble mozaiğinden küre görüntüsünü üretme (`web/public/globe/earth.jpg` + 128 adet 1024 px döşeme) |
| `cvd_check.py` | Renk paletlerini renk körlüğü simülasyonu (Machado 2009), CIEDE2000 ve WCAG kontrastıyla denetler; docs/RENK_DENETIMI.md üretir (Python 3 + numpy gerekir, sh değil) |
| `static-mask-check.sh` | Statik kaynak (gaz meşalesi) maske eşiğini ve SAMA gece filtresini yerel FIRMS arşivinden ölçer; `crates/firms-client/examples/static_mask_check.rs` örneğini çalıştırır |
| `build_biome_grid.py` | WPS `modis_landuse_20class_5m_with_lakes` (MODIS IGBP, 5') verisinden `data/landcover/biome_0p25.rle` ve `SOURCE.md` üretir (`--download` veya `--source`; Python 3 stdlib, numpy varsa kullanılır, sh değil) |
| `firecci_extract.py` | ESA Fire_cci v5.1 0.25° aylık grid zip'lerini (`.cache/firecci/zip`, CEDA, girişsiz) `burned_area > 0` hücre-ay CSV'sine düzleştirir; Y10 `burned_area_validation` örneğinin girdisi (Python 3 + numpy + scipy, `python -I` ile çalıştırılır, sh değil) |
| `smoke_test.sh` | Çalışan PyroSync sunucusuna karşı uçtan uca duman testlerini çalıştırır (`BASE_URL`, varsayılan `http://localhost:3001`) |

## Script Ekleme Kuralları

- Her script dosya başında kısa bir açıklama yorumu bulundurur.
- Kullanım bilgisi (usage) script içinde belirtilir.
- Hata durumunda anlaşılır mesaj verilir ve sıfırdan farklı exit code döndürülür.
