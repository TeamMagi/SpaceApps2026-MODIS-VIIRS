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

## Script Ekleme Kuralları

- Her script dosya başında kısa bir açıklama yorumu bulundurur.
- Kullanım bilgisi (usage) script içinde belirtilir.
- Hata durumunda anlaşılır mesaj verilir ve sıfırdan farklı exit code döndürülür.
