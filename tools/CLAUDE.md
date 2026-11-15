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

## Script Ekleme Kuralları

- Her script dosya başında kısa bir açıklama yorumu bulundurur.
- Kullanım bilgisi (usage) script içinde belirtilir.
- Hata durumunda anlaşılır mesaj verilir ve sıfırdan farklı exit code döndürülür.
