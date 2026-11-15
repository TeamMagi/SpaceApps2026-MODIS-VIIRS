# Renk Denetimi (C7)

Bu rapor `web/src/styles/tokens.css` içindeki renklerin renk körlüğü altında ayırt edilebilirliğini denetler: sensör renkleri (UI + harita), durum renkleri, FRP sıralı skalası, anomali sapma skalası, vurgu/durum renklerinin zemin kontrastı. Koyu ve açık tema ayrı ayrı değerlendirilir.

**Yeniden üretme:** `python tools/cvd_check.py --out docs/RENK_DENETIMI.md` (Python 3 + numpy; `--self-test` CIEDE2000 gerçeklemesini doğrular; mevcut token'larda FAIL varsa çıkış kodu 1). Bu dosya elle düzenlenmez.

**Yöntem:** Protanopi, deuteranopi ve tritanopi, Machado, Oliveira & Fernandes (2009) matrisleriyle (şiddet 1.0) doğrusal RGB üzerinde simüle edilir (sRGB gama çözme/kodlama dahil). Akromatopsi, yalnızca parlaklığa indirgenmiş gri tondur (baskı/gri kontrol). Renk farkı CIEDE2000 (ΔE00), kontrast WCAG 2.x göreli parlaklık oranıdır. Eşikler: kategorik ΔE00 >= 10 (WARN 6-10), sıralı komşu adım ΔE00 >= 5 (WARN 3-5), grafik/UI kontrastı >= 3:1. Akromatopsi kategorik ve sapma çiftlerinde yalnızca bilgi amaçlıdır (bu setler ton ile kodlanır; durum rengi her zaman metinle, sensör ise şekille (MODIS içi boş daire, VIIRS dolu kare, `docs/TASARIM_SISTEMI.md`) eşlenir); sıralı skalada her durumda sayılır.

**Özet (mevcut token'lar):** 2 FAIL, 3 WARN, 43 PASS. Aday paletler (viridis/cividis) sayıma girmez.

| Tema | Grup | Öğe | Durum | Ayrıntı |
|---|---|---|---|---|
| dark | Sıralı FRP | contrast vs map-land (>=3 desired) | **FAIL** | min 1.20; max 13.47; below 3:1: 1=1.20, 2=1.64, 3=2.27 |
| light | Sıralı FRP | contrast vs map-land (>=3 desired) | **FAIL** | min 1.13; max 13.46; below 3:1: 1=1.13, 2=1.42, 3=1.98, 4=2.98 |
| dark | Kategorik: sensör UI | color-sensor-modis / color-duplicate-ring | **WARN** | dE00: normal 6.2, protanopi 6.2, deuteranopi 6.5, tritanopi 6.3, akromatopsi 5.7 |
| light | Kategorik: durum | color-status-demo / color-status-error | **WARN** | dE00: normal 27.2, protanopi 24.0, deuteranopi 16.2, tritanopi 9.6, akromatopsi 11.3 |
| light | Sapma (anomali) | anom-0 vs anom-none (>=10) | **WARN** | dE00: normal 6.6, protanopi 6.5, deuteranopi 6.9, tritanopi 6.6, akromatopsi 5.3 |

## Tema: dark

Zemin: color-bg `#0d0f13`, color-surface `#14171d`, map-land `#151922`, map-water `#070a0f`.

### FRP skalası (normal ve simüle hex)

| Token | normal | protanopi | deuteranopi | tritanopi | akromatopsi |
|---|---|---|---|---|---|
| `frp-1` | `#3d0f5e` | `#002760` | `#00285c` | `#372436` | `#282828` |
| `frp-2` | `#6a176e` | `#003770` | `#2c416c` | `#6d2642` | `#3e3e3e` |
| `frp-3` | `#932667` | `#364669` | `#565a65` | `#9e2143` | `#535353` |
| `frp-4` | `#bc3754` | `#585754` | `#7b7351` | `#cd1843` | `#696969` |
| `frp-5` | `#dd513a` | `#786d37` | `#9b8b35` | `#f32d4c` | `#808080` |
| `frp-6` | `#f37819` | `#9c8900` | `#baa514` | `#ff5c68` | `#9b9b9b` |
| `frp-7` | `#fcb519` | `#d0b700` | `#e1c924` | `#ffa19b` | `#c2c2c2` |
| `frp-8` | `#f6e27a` | `#f4dd71` | `#f9e47e` | `#ffd5ca` | `#e1e1e1` |

### Anomali skalası (normal ve simüle hex)

| Token | normal | protanopi | deuteranopi | tritanopi | akromatopsi |
|---|---|---|---|---|---|
| `anom-neg-3` | `#6a51a3` | `#3360a6` | `#395ea1` | `#5d6172` | `#606060` |
| `anom-neg-2` | `#8c7cc0` | `#6a87c3` | `#6c85be` | `#818895` | `#868686` |
| `anom-neg-1` | `#b8afd8` | `#a6b5da` | `#a7b4d7` | `#b2b5bd` | `#b4b4b4` |
| `anom-0` | `#333b4d` | `#353c4e` | `#333a4d` | `#2c3f41` | `#3b3b3b` |
| `anom-pos-1` | `#f6c27a` | `#d5c474` | `#e2d07c` | `#ffb4b0` | `#cbcbcb` |
| `anom-pos-2` | `#e68a2a` | `#a69318` | `#bca82b` | `#fc7578` | `#a0a0a0` |
| `anom-pos-3` | `#b85a0d` | `#766700` | `#8c7c09` | `#ca444d` | `#747474` |
| `anom-none` | `#161a24` | `#171b24` | `#161a24` | `#121c1d` | `#1a1a1a` |

### Kategorik: sensör UI

| Öğe | Durum | Ayrıntı |
|---|---|---|
| color-sensor-modis / color-sensor-viirs | PASS | dE00: normal 18.5, protanopi 18.1, deuteranopi 18.8, tritanopi 18.5, akromatopsi 18.4 |
| color-sensor-modis / color-duplicate-ring | WARN | dE00: normal 6.2, protanopi 6.2, deuteranopi 6.5, tritanopi 6.3, akromatopsi 5.7 |
| color-sensor-viirs / color-duplicate-ring | PASS | dE00: normal 24.5, protanopi 24.0, deuteranopi 25.0, tritanopi 24.5, akromatopsi 23.9 |

### Kategorik: harita sensörleri

| Öğe | Durum | Ayrıntı |
|---|---|---|
| map-modis / map-viirs | PASS | dE00: normal 18.6, protanopi 18.1, deuteranopi 18.8, tritanopi 18.4, akromatopsi 18.1 |
| map-modis / map-duplicate-ring | PASS | dE00: normal 21.2, protanopi 21.0, deuteranopi 22.2, tritanopi 24.1, akromatopsi 3.8 |
| map-viirs / map-duplicate-ring | PASS | dE00: normal 29.2, protanopi 28.8, deuteranopi 32.0, tritanopi 33.6, akromatopsi 14.3 |

### Kategorik: durum

| Öğe | Durum | Ayrıntı |
|---|---|---|
| color-status-live / color-status-demo | PASS | dE00: normal 41.8, protanopi 25.7, deuteranopi 34.8, tritanopi 51.6, akromatopsi 2.5 |
| color-status-live / color-status-error | PASS | dE00: normal 66.2, protanopi 17.1, deuteranopi 20.8, tritanopi 68.5, akromatopsi 9.1 |
| color-status-demo / color-status-error | PASS | dE00: normal 46.2, protanopi 30.1, deuteranopi 19.9, tritanopi 17.0, akromatopsi 11.6 |

### Sıralı FRP

| Öğe | Durum | Ayrıntı |
|---|---|---|
| L* strictly monotonic | PASS | normal ↑, protanopi ↑, deuteranopi ↑, tritanopi ↑, akromatopsi ↑; normal L*: 16, 26, 35, 44, 54, 64, 78, 90 |
| min adjacent dE00 (>=5) | PASS | normal 9.2 (1-2), protanopi 5.5 (1-2), deuteranopi 8.1 (1-2), tritanopi 7.9 (5-6), akromatopsi 7.1 (1-2) |
| contrast vs map-land (>=3 desired) | FAIL | min 1.20; max 13.47; below 3:1: 1=1.20, 2=1.64, 3=2.27 |

### Sapma (anomali)

| Öğe | Durum | Ayrıntı |
|---|---|---|
| neg arm L* monotonic (step 1->3) | PASS | normal L*: 73, 56, 41 |
| pos arm L* monotonic (step 1->3) | PASS | normal L*: 82, 66, 49 |
| neg-1 vs pos-1 (>=10) | PASS | dE00: normal 38.5, protanopi 39.1, deuteranopi 39.4, tritanopi 23.1, akromatopsi 6.0 |
| neg-2 vs pos-2 (>=10) | PASS | dE00: normal 46.9, protanopi 50.4, deuteranopi 51.4, tritanopi 30.9, akromatopsi 8.6 |
| neg-3 vs pos-3 (>=10) | PASS | dE00: normal 45.8, protanopi 51.0, deuteranopi 53.2, tritanopi 30.3, akromatopsi 7.7 |
| anom-0 vs anom-none (>=10) | PASS | dE00: normal 10.9, protanopi 11.2, deuteranopi 10.8, tritanopi 11.1, akromatopsi 10.5 |
| centre lightness note | PASS | anom-0 L* 25; neg-1 73, pos-1 82 (centre is the 'quiet' end against the map; arms are checked from step 1) |

### Metin/UI kontrastı

| Öğe | Durum | Ayrıntı |
|---|---|---|
| color-accent | PASS | vs color-bg 9.78, vs color-surface 9.16 (>=3:1) |
| color-accent-strong | PASS | vs color-bg 11.94, vs color-surface 11.17 (>=3:1) |
| color-status-live | PASS | vs color-bg 10.30, vs color-surface 9.64 (>=3:1) |
| color-status-demo | PASS | vs color-bg 11.49, vs color-surface 10.75 (>=3:1) |
| color-status-error | PASS | vs color-bg 7.13, vs color-surface 6.67 (>=3:1) |

## Tema: light

Zemin: color-bg `#f6f7f9`, color-surface `#ffffff`, map-land `#f3f5f8`, map-water `#dfe5ee`.

### FRP skalası (normal ve simüle hex)

| Token | normal | protanopi | deuteranopi | tritanopi | akromatopsi |
|---|---|---|---|---|---|
| `frp-1` | `#fbe7a1` | `#f5e49c` | `#fbeaa3` | `#ffddd5` | `#e7e7e7` |
| `frp-2` | `#fcc85a` | `#dfc84e` | `#ebd55e` | `#ffb7b0` | `#d0d0d0` |
| `frp-3` | `#f89b2c` | `#b9a314` | `#cfb92e` | `#ff8586` | `#b0b0b0` |
| `frp-4` | `#e8682a` | `#8c7d20` | `#ac9925` | `#ff4a5c` | `#8e8e8e` |
| `frp-5` | `#c8413f` | `#665e3e` | `#887b3b` | `#dc1b42` | `#717171` |
| `frp-6` | `#9c2a5c` | `#41495d` | `#605f59` | `#a91b3f` | `#575757` |
| `frp-7` | `#6e1c6c` | `#103a6e` | `#33446a` | `#722742` | `#414141` |
| `frp-8` | `#3d0f5e` | `#002760` | `#00285c` | `#372436` | `#282828` |

### Anomali skalası (normal ve simüle hex)

| Token | normal | protanopi | deuteranopi | tritanopi | akromatopsi |
|---|---|---|---|---|---|
| `anom-neg-3` | `#542788` | `#00408b` | `#003f86` | `#493e54` | `#404040` |
| `anom-neg-2` | `#8073ac` | `#657cae` | `#667aaa` | `#777d88` | `#7b7b7b` |
| `anom-neg-1` | `#c7c2e0` | `#bcc6e1` | `#bcc5df` | `#c2c6cc` | `#c5c5c5` |
| `anom-0` | `#dce1ea` | `#dee1ea` | `#dde0ea` | `#d9e3e4` | `#e1e1e1` |
| `anom-pos-1` | `#fdc983` | `#dccb7d` | `#e9d785` | `#ffbcb8` | `#d2d2d2` |
| `anom-pos-2` | `#e08214` | `#9f8c00` | `#b5a215` | `#f66c6f` | `#999999` |
| `anom-pos-3` | `#a14e05` | `#665900` | `#7a6c02` | `#b13b42` | `#656565` |
| `anom-none` | `#f9fafb` | `#fafafb` | `#f9fafb` | `#f9fafa` | `#fafafa` |

### Kategorik: sensör UI

| Öğe | Durum | Ayrıntı |
|---|---|---|
| color-sensor-modis / color-sensor-viirs | PASS | dE00: normal 30.8, protanopi 31.1, deuteranopi 30.5, tritanopi 30.8, akromatopsi 30.5 |
| color-sensor-modis / color-duplicate-ring | PASS | dE00: normal 14.5, protanopi 14.8, deuteranopi 14.4, tritanopi 14.6, akromatopsi 14.0 |
| color-sensor-viirs / color-duplicate-ring | PASS | dE00: normal 43.6, protanopi 44.1, deuteranopi 43.3, tritanopi 43.7, akromatopsi 43.2 |

### Kategorik: harita sensörleri

| Öğe | Durum | Ayrıntı |
|---|---|---|
| map-modis / map-viirs | PASS | dE00: normal 21.5, protanopi 21.7, deuteranopi 21.3, tritanopi 21.7, akromatopsi 21.3 |
| map-modis / map-duplicate-ring | PASS | dE00: normal 10.6, protanopi 10.9, deuteranopi 10.5, tritanopi 10.7, akromatopsi 9.8 |
| map-viirs / map-duplicate-ring | PASS | dE00: normal 31.1, protanopi 31.7, deuteranopi 30.9, tritanopi 31.6, akromatopsi 30.7 |

### Kategorik: durum

| Öğe | Durum | Ayrıntı |
|---|---|---|
| color-status-live / color-status-demo | PASS | dE00: normal 45.6, protanopi 23.4, deuteranopi 32.2, tritanopi 55.1, akromatopsi 2.3 |
| color-status-live / color-status-error | PASS | dE00: normal 56.7, protanopi 17.4, deuteranopi 22.0, tritanopi 56.7, akromatopsi 9.0 |
| color-status-demo / color-status-error | WARN | dE00: normal 27.2, protanopi 24.0, deuteranopi 16.2, tritanopi 9.6, akromatopsi 11.3 |

### Sıralı FRP

| Öğe | Durum | Ayrıntı |
|---|---|---|
| L* strictly monotonic | PASS | normal ↓, protanopi ↓, deuteranopi ↓, tritanopi ↓, akromatopsi ↓; normal L*: 92, 83, 72, 59, 48, 37, 27, 16 |
| min adjacent dE00 (>=5) | PASS | normal 10.5 (7-8), protanopi 6.2 (7-8), deuteranopi 7.3 (2-3), tritanopi 10.7 (3-4), akromatopsi 5.2 (1-2) |
| contrast vs map-land (>=3 desired) | FAIL | min 1.13; max 13.46; below 3:1: 1=1.13, 2=1.42, 3=1.98, 4=2.98 |

### Sapma (anomali)

| Öğe | Durum | Ayrıntı |
|---|---|---|
| neg arm L* monotonic (step 1->3) | PASS | normal L*: 80, 52, 27 |
| pos arm L* monotonic (step 1->3) | PASS | normal L*: 84, 63, 43 |
| neg-1 vs pos-1 (>=10) | PASS | dE00: normal 34.4, protanopi 35.0, deuteranopi 35.4, tritanopi 22.2, akromatopsi 3.2 |
| neg-2 vs pos-2 (>=10) | PASS | dE00: normal 46.3, protanopi 48.6, deuteranopi 50.6, tritanopi 31.2, akromatopsi 10.6 |
| neg-3 vs pos-3 (>=10) | PASS | dE00: normal 47.9, protanopi 52.6, deuteranopi 55.2, tritanopi 27.6, akromatopsi 12.9 |
| anom-0 vs anom-none (>=10) | WARN | dE00: normal 6.6, protanopi 6.5, deuteranopi 6.9, tritanopi 6.6, akromatopsi 5.3 |
| centre lightness note | PASS | anom-0 L* 89; neg-1 80, pos-1 84 (centre is the 'quiet' end against the map; arms are checked from step 1) |

### Metin/UI kontrastı

| Öğe | Durum | Ayrıntı |
|---|---|---|
| color-accent | PASS | vs color-bg 5.23, vs color-surface 5.60 (>=3:1) |
| color-accent-strong | PASS | vs color-bg 7.49, vs color-surface 8.03 (>=3:1) |
| color-status-live | PASS | vs color-bg 5.11, vs color-surface 5.47 (>=3:1) |
| color-status-demo | PASS | vs color-bg 4.68, vs color-surface 5.02 (>=3:1) |
| color-status-error | PASS | vs color-bg 7.48, vs color-surface 8.02 (>=3:1) |

## Aday sıralı paletler (K4 renk körlüğü dostu seçenek)

Kaynak: viridis, matplotlib 'viridis' haritasının 8 eşit aralıklı örneği; cividis, Nuñez, Anderton & Renslow (2018), matplotlib 'cividis' 10 çıpa değerinden 8 örneğe doğrusal interpolasyonla türetilmiştir.

- viridis: `#440154` `#46327e` `#365c8d` `#277f8e` `#1fa187` `#4ac16d` `#9fda3a` `#fde725`
- cividis: `#00204d` `#10396e` `#4a536c` `#6c6e72` `#8e8a78` `#b3a771` `#dac561` `#ffea46`

| Tema | Palet | Karar | Önerilen yön (düşük → yüksek) | Kontrast: doğal 1. adım (koyu uç) | Kontrast: doğal 8. adım (açık uç) |
|---|---|---|---|---|---|
| dark | viridis | uygun | koyudan açığa (1=koyu, 8=açık) | 1.15 | 13.93 |
| dark | cividis | uygun | koyudan açığa (1=koyu, 8=açık) | 1.10 | 14.33 |
| light | viridis | uygun | açıktan koyuya (1=açık, 8=koyu) | 13.95 | 1.16 |
| light | cividis | uygun | açıktan koyuya (1=açık, 8=koyu) | 14.64 | 1.12 |

Karar: L* tüm simülasyonlarda monoton, komşu ΔE00 FAIL değil ve sıcak uç map-land'a karşı >= 3:1 ise 'uygun'. Düşük uç zeminle kaynaşabilir (düşük FRP için kabul edilebilir); ayrıntılar aşağıda.

### dark / viridis

| Öğe | Durum | Ayrıntı |
|---|---|---|
| L* strictly monotonic | PASS | normal ↑, protanopi ↑, deuteranopi ↑, tritanopi ↑, akromatopsi ↑; normal L*: 15, 27, 38, 49, 59, 70, 81, 91 |
| min adjacent dE00 (>=5) | PASS | normal 12.2 (1-2), protanopi 5.3 (7-8), deuteranopi 8.5 (1-2), tritanopi 8.0 (5-6), akromatopsi 6.7 (7-8) |
| contrast vs map-land (>=3 desired) | FAIL | min 1.15; max 13.93; below 3:1: 1=1.15, 2=1.69, 3=2.57 |

### dark / cividis

| Öğe | Durum | Ayrıntı |
|---|---|---|
| L* strictly monotonic | PASS | normal ↑, protanopi ↑, deuteranopi ↑, tritanopi ↑, akromatopsi ↑; normal L*: 13, 24, 35, 46, 57, 68, 79, 92 |
| min adjacent dE00 (>=5) | PASS | normal 8.4 (1-2), protanopi 8.2 (1-2), deuteranopi 8.3 (1-2), tritanopi 7.9 (7-8), akromatopsi 7.7 (1-2) |
| contrast vs map-land (>=3 desired) | FAIL | min 1.10; max 14.33; below 3:1: 1=1.10, 2=1.53, 3=2.30 |

### light / viridis

| Öğe | Durum | Ayrıntı |
|---|---|---|
| L* strictly monotonic | PASS | normal ↑, protanopi ↑, deuteranopi ↑, tritanopi ↑, akromatopsi ↑; normal L*: 15, 27, 38, 49, 59, 70, 81, 91 |
| min adjacent dE00 (>=5) | PASS | normal 12.2 (1-2), protanopi 5.3 (7-8), deuteranopi 8.5 (1-2), tritanopi 8.0 (5-6), akromatopsi 6.7 (7-8) |
| contrast vs map-land (>=3 desired) | FAIL | min 1.16; max 13.95; below 3:1: 5=2.96, 6=2.10, 7=1.53, 8=1.16 |

### light / cividis

| Öğe | Durum | Ayrıntı |
|---|---|---|
| L* strictly monotonic | PASS | normal ↑, protanopi ↑, deuteranopi ↑, tritanopi ↑, akromatopsi ↑; normal L*: 13, 24, 35, 46, 57, 68, 79, 92 |
| min adjacent dE00 (>=5) | PASS | normal 8.4 (1-2), protanopi 8.2 (1-2), deuteranopi 8.3 (1-2), tritanopi 7.9 (7-8), akromatopsi 7.7 (1-2) |
| contrast vs map-land (>=3 desired) | FAIL | min 1.12; max 14.64; below 3:1: 6=2.21, 7=1.58, 8=1.12 |

