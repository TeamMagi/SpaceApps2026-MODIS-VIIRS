# Biyom bazli tamlik katsayisi k_c (K14 olcumu)

Aqua MODIS (`MODIS_SP`, yalniz Aqua satirlari) ve S-NPP 375 m VIIRS (`VIIRS_SNPP_SP`), 1x1 derece UTC hucre-gun, |dt| <= 20 dk, gunduz ve gece ayri. k = toplam(VIIRS FRP) / toplam(MODIS FRP x R_rel(theta)). Uydurma yil 2019, hold-out yil 2020. Asgari ornek (hucre-gun) 30.

Onbellek: 1574 pencere onbellekten, 1954 pencere agdan; harcanan islem 9735.

FIRMS sayacindan gozlenen gercek islem: 12880 (yukaridaki rakam pencere basina gun sayisina dayanan tahmindir ve gercek maliyetin altindadir).
- MODIS_SP: 6.1 islem/pencere (5156 islem, 840 pencere)
- VIIRS_SNPP_SP: 8.3 islem/pencere (7724 islem, 933 pencere)

## Bolgeler

| Bolge | bbox (w,s,e,n) | Aqua satir | VIIRS satir | cift hucre-gun 2019 | cift hucre-gun 2020 |
|---|---|---:|---:|---:|---:|
| cerrado | -55,-17,-43,-7 | 96704 | 731903 | 4816 | 3954 |
| s_africa_savanna | 14,-17,30,-8 | 683657 | 3703483 | 7994 | 6858 |
| n_africa_savanna | 10,5,30,13 | 391829 | 2553445 | 5599 | 6606 |
| n_australia | 126,-18,142,-11 | 69357 | 566652 | 2409 | 1720 |
| se_asia | 97,14,107,22 | 140697 | 1010433 | 3246 | 3130 |
| punjab | 73,28.5,77.5,32.5 | 34808 | 205771 | 480 | 490 |
| ukraine_russia | 30,46,45,52 | 23868 | 137158 | 1714 | 1923 |
| kazakh_steppe | 60,47,78,53 | 12415 | 99392 | 735 | 423 |
| siberia_boreal | 90,55,110,62 | 37057 | 432440 | 1836 | 667 |
| turkiye_e_med | 26,36,45,42 | 11938 | 103287 | 1220 | 932 |
| western_usa | -124,34,-110,44 | 27239 | 254558 | 672 | 1209 |
| amazon_arc | -62,-12,-48,-5 | 79997 | 579211 | 3246 | 2823 |

Disarida birakilan bolge yok.

## k_c tablosu (uydurma yili 2019)

sigma = hucre-gunler uzerinden oran-toplam kestiriminin standart hatasi (delta yontemi); `sd_hg` = hucre-gun orani standart sapmasi (yalniz bilgi).

| Biyom | Seviye | Ay | Gun/gece | k_c | sigma | sd_hg | N |
|---|---|---:|---|---:|---:|---:|---:|
| forest | biyom | - | hepsi | 1.091 | 0.016 | 2.729 | 8626 |
| forest | biyom-gun/gece | - | gece | 1.998 | 0.049 | 3.260 | 1861 |
| forest | biyom-gun/gece | - | gunduz | 1.048 | 0.016 | 2.485 | 6765 |
| forest | biyom-ay-gun/gece | 1 | gunduz | 1.176 | 0.071 | 1.617 | 237 |
| forest | biyom-ay-gun/gece | 2 | gece | 3.144 | 0.624 | 4.890 | 48 |
| forest | biyom-ay-gun/gece | 2 | gunduz | 1.062 | 0.039 | 2.701 | 421 |
| forest | biyom-ay-gun/gece | 3 | gece | 3.168 | 0.281 | 4.501 | 178 |
| forest | biyom-ay-gun/gece | 3 | gunduz | 0.946 | 0.034 | 1.698 | 558 |
| forest | biyom-ay-gun/gece | 4 | gece | 2.822 | 0.225 | 4.041 | 139 |
| forest | biyom-ay-gun/gece | 4 | gunduz | 0.914 | 0.058 | 3.253 | 451 |
| forest | biyom-ay-gun/gece | 5 | gece | 1.975 | 0.217 | 2.537 | 84 |
| forest | biyom-ay-gun/gece | 5 | gunduz | 1.260 | 0.077 | 2.237 | 428 |
| forest | biyom-ay-gun/gece | 6 | gece | 1.403 | 0.112 | 1.124 | 95 |
| forest | biyom-ay-gun/gece | 6 | gunduz | 1.175 | 0.050 | 1.358 | 443 |
| forest | biyom-ay-gun/gece | 7 | gece | 2.044 | 0.093 | 4.031 | 405 |
| forest | biyom-ay-gun/gece | 7 | gunduz | 1.158 | 0.027 | 2.336 | 1168 |
| forest | biyom-ay-gun/gece | 8 | gece | 2.052 | 0.068 | 2.480 | 506 |
| forest | biyom-ay-gun/gece | 8 | gunduz | 1.030 | 0.027 | 1.818 | 1296 |
| forest | biyom-ay-gun/gece | 9 | gece | 1.750 | 0.062 | 1.770 | 290 |
| forest | biyom-ay-gun/gece | 9 | gunduz | 1.096 | 0.031 | 3.835 | 851 |
| forest | biyom-ay-gun/gece | 10 | gece | 1.590 | 0.418 | 1.800 | 53 |
| forest | biyom-ay-gun/gece | 10 | gunduz | 1.253 | 0.071 | 3.315 | 388 |
| forest | biyom-ay-gun/gece | 11 | gunduz | 0.923 | 0.062 | 1.733 | 251 |
| forest | biyom-ay-gun/gece | 12 | gunduz | 1.040 | 0.062 | 1.093 | 273 |
| shrubland | biyom | - | hepsi | 1.267 | 0.040 | 2.376 | 1454 |
| shrubland | biyom-gun/gece | - | gece | 1.565 | 0.075 | 2.184 | 308 |
| shrubland | biyom-gun/gece | - | gunduz | 1.244 | 0.042 | 2.420 | 1146 |
| shrubland | biyom-ay-gun/gece | 1 | gunduz | 0.923 | 0.155 | 2.620 | 30 |
| shrubland | biyom-ay-gun/gece | 2 | gunduz | 1.374 | 0.183 | 1.820 | 30 |
| shrubland | biyom-ay-gun/gece | 3 | gunduz | 1.145 | 0.133 | 2.189 | 53 |
| shrubland | biyom-ay-gun/gece | 4 | gunduz | 1.111 | 0.118 | 1.582 | 46 |
| shrubland | biyom-ay-gun/gece | 5 | gunduz | 1.480 | 0.183 | 2.323 | 86 |
| shrubland | biyom-ay-gun/gece | 6 | gunduz | 1.127 | 0.138 | 1.106 | 110 |
| shrubland | biyom-ay-gun/gece | 7 | gece | 1.687 | 0.148 | 1.566 | 57 |
| shrubland | biyom-ay-gun/gece | 7 | gunduz | 1.307 | 0.092 | 2.666 | 156 |
| shrubland | biyom-ay-gun/gece | 8 | gece | 1.959 | 0.292 | 3.118 | 62 |
| shrubland | biyom-ay-gun/gece | 8 | gunduz | 1.035 | 0.118 | 1.827 | 138 |
| shrubland | biyom-ay-gun/gece | 9 | gece | 1.470 | 0.114 | 1.295 | 34 |
| shrubland | biyom-ay-gun/gece | 9 | gunduz | 1.269 | 0.063 | 2.327 | 138 |
| shrubland | biyom-ay-gun/gece | 10 | gunduz | 1.291 | 0.097 | 2.773 | 148 |
| shrubland | biyom-ay-gun/gece | 11 | gece | 1.409 | 0.141 | 2.582 | 30 |
| shrubland | biyom-ay-gun/gece | 11 | gunduz | 1.487 | 0.138 | 3.503 | 143 |
| shrubland | biyom-ay-gun/gece | 12 | gunduz | 1.302 | 0.144 | 1.483 | 68 |
| savanna | biyom | - | hepsi | 1.193 | 0.008 | 2.291 | 17468 |
| savanna | biyom-gun/gece | - | gece | 1.319 | 0.037 | 2.052 | 1658 |
| savanna | biyom-gun/gece | - | gunduz | 1.192 | 0.008 | 2.314 | 15810 |
| savanna | biyom-ay-gun/gece | 1 | gece | 1.476 | 0.171 | 2.768 | 104 |
| savanna | biyom-ay-gun/gece | 1 | gunduz | 1.332 | 0.029 | 2.539 | 1141 |
| savanna | biyom-ay-gun/gece | 2 | gece | 1.638 | 0.312 | 4.118 | 62 |
| savanna | biyom-ay-gun/gece | 2 | gunduz | 1.231 | 0.055 | 2.500 | 802 |
| savanna | biyom-ay-gun/gece | 3 | gece | 1.112 | 0.205 | 2.750 | 38 |
| savanna | biyom-ay-gun/gece | 3 | gunduz | 1.431 | 0.051 | 2.659 | 718 |
| savanna | biyom-ay-gun/gece | 4 | gunduz | 1.290 | 0.067 | 2.213 | 469 |
| savanna | biyom-ay-gun/gece | 5 | gece | 1.532 | 0.240 | 1.726 | 51 |
| savanna | biyom-ay-gun/gece | 5 | gunduz | 1.265 | 0.027 | 2.149 | 1265 |
| savanna | biyom-ay-gun/gece | 6 | gece | 1.114 | 0.100 | 1.315 | 124 |
| savanna | biyom-ay-gun/gece | 6 | gunduz | 1.145 | 0.017 | 1.624 | 1878 |
| savanna | biyom-ay-gun/gece | 7 | gece | 1.099 | 0.118 | 1.226 | 184 |
| savanna | biyom-ay-gun/gece | 7 | gunduz | 1.084 | 0.016 | 1.545 | 2025 |
| savanna | biyom-ay-gun/gece | 8 | gece | 1.471 | 0.073 | 1.600 | 324 |
| savanna | biyom-ay-gun/gece | 8 | gunduz | 1.203 | 0.019 | 3.233 | 2082 |
| savanna | biyom-ay-gun/gece | 9 | gece | 1.403 | 0.070 | 2.078 | 372 |
| savanna | biyom-ay-gun/gece | 9 | gunduz | 1.257 | 0.021 | 2.139 | 1935 |
| savanna | biyom-ay-gun/gece | 10 | gece | 1.422 | 0.052 | 1.926 | 205 |
| savanna | biyom-ay-gun/gece | 10 | gunduz | 1.265 | 0.044 | 2.838 | 1195 |
| savanna | biyom-ay-gun/gece | 11 | gece | 1.090 | 0.084 | 1.810 | 81 |
| savanna | biyom-ay-gun/gece | 11 | gunduz | 1.257 | 0.030 | 2.056 | 1034 |
| savanna | biyom-ay-gun/gece | 12 | gece | 0.949 | 0.160 | 2.416 | 92 |
| savanna | biyom-ay-gun/gece | 12 | gunduz | 1.155 | 0.024 | 1.645 | 1266 |
| grassland | biyom | - | hepsi | 1.224 | 0.031 | 3.882 | 2146 |
| grassland | biyom-gun/gece | - | gece | 1.332 | 0.141 | 2.685 | 251 |
| grassland | biyom-gun/gece | - | gunduz | 1.217 | 0.032 | 4.013 | 1895 |
| grassland | biyom-ay-gun/gece | 1 | gunduz | 1.291 | 0.080 | 1.610 | 180 |
| grassland | biyom-ay-gun/gece | 2 | gunduz | 1.284 | 0.123 | 1.672 | 116 |
| grassland | biyom-ay-gun/gece | 3 | gunduz | 1.457 | 0.149 | 2.493 | 126 |
| grassland | biyom-ay-gun/gece | 4 | gunduz | 1.219 | 0.094 | 3.982 | 127 |
| grassland | biyom-ay-gun/gece | 5 | gunduz | 1.425 | 0.247 | 5.940 | 103 |
| grassland | biyom-ay-gun/gece | 6 | gunduz | 0.959 | 0.099 | 1.744 | 90 |
| grassland | biyom-ay-gun/gece | 7 | gece | 1.525 | 0.280 | 3.960 | 64 |
| grassland | biyom-ay-gun/gece | 7 | gunduz | 1.342 | 0.114 | 8.973 | 191 |
| grassland | biyom-ay-gun/gece | 8 | gece | 1.072 | 0.231 | 1.371 | 44 |
| grassland | biyom-ay-gun/gece | 8 | gunduz | 0.976 | 0.067 | 5.926 | 146 |
| grassland | biyom-ay-gun/gece | 9 | gece | 1.273 | 0.156 | 0.794 | 30 |
| grassland | biyom-ay-gun/gece | 9 | gunduz | 1.160 | 0.068 | 2.016 | 121 |
| grassland | biyom-ay-gun/gece | 10 | gunduz | 1.036 | 0.083 | 2.018 | 185 |
| grassland | biyom-ay-gun/gece | 11 | gunduz | 1.308 | 0.058 | 1.323 | 302 |
| grassland | biyom-ay-gun/gece | 12 | gece | 1.318 | 0.230 | 1.946 | 31 |
| grassland | biyom-ay-gun/gece | 12 | gunduz | 1.204 | 0.062 | 1.656 | 208 |
| cropland | biyom | - | hepsi | 1.325 | 0.026 | 3.045 | 4265 |
| cropland | biyom-gun/gece | - | gece | 1.487 | 0.148 | 2.099 | 313 |
| cropland | biyom-gun/gece | - | gunduz | 1.321 | 0.026 | 3.107 | 3952 |
| cropland | biyom-ay-gun/gece | 1 | gece | 2.103 | 0.294 | 3.408 | 32 |
| cropland | biyom-ay-gun/gece | 1 | gunduz | 1.171 | 0.064 | 1.692 | 158 |
| cropland | biyom-ay-gun/gece | 2 | gece | 2.592 | 0.485 | 2.086 | 40 |
| cropland | biyom-ay-gun/gece | 2 | gunduz | 1.480 | 0.084 | 2.595 | 188 |
| cropland | biyom-ay-gun/gece | 3 | gece | 1.421 | 0.343 | 2.294 | 38 |
| cropland | biyom-ay-gun/gece | 3 | gunduz | 1.331 | 0.062 | 2.046 | 318 |
| cropland | biyom-ay-gun/gece | 4 | gunduz | 1.260 | 0.062 | 3.335 | 517 |
| cropland | biyom-ay-gun/gece | 5 | gunduz | 1.751 | 0.113 | 3.936 | 288 |
| cropland | biyom-ay-gun/gece | 6 | gunduz | 1.462 | 0.164 | 5.604 | 116 |
| cropland | biyom-ay-gun/gece | 7 | gece | 1.279 | 0.157 | 1.620 | 38 |
| cropland | biyom-ay-gun/gece | 7 | gunduz | 1.170 | 0.095 | 4.106 | 324 |
| cropland | biyom-ay-gun/gece | 8 | gece | 1.794 | 0.517 | 1.963 | 40 |
| cropland | biyom-ay-gun/gece | 8 | gunduz | 1.199 | 0.085 | 4.919 | 337 |
| cropland | biyom-ay-gun/gece | 9 | gece | 0.990 | 0.170 | 1.199 | 31 |
| cropland | biyom-ay-gun/gece | 9 | gunduz | 1.148 | 0.074 | 1.938 | 462 |
| cropland | biyom-ay-gun/gece | 10 | gunduz | 1.207 | 0.073 | 2.366 | 657 |
| cropland | biyom-ay-gun/gece | 11 | gunduz | 1.368 | 0.067 | 1.663 | 378 |
| cropland | biyom-ay-gun/gece | 12 | gunduz | 1.233 | 0.097 | 1.850 | 209 |

## Guc yasasi (VIIRS FRP, uydurma yili)

alpha: p(x) ~ x^-alpha icin Hill en-olabilirlik kestirimi, x >= 4.96 MW; SE = (alpha-1)/sqrt(N). `p_max` = tum VIIRS FRP degerlerinin 99.9. yuzdeligi. Literatur (Cerrado, [15, Sek. 6]) 1.31.

| Biyom | alpha | SE | N (kuyruk) | N (tum) | p_max (MW) |
|---|---:|---:|---:|---:|---:|
| forest | 2.087 | 0.001 | 635748 | 1323353 | 285.7 |
| shrubland | 2.145 | 0.004 | 75849 | 145098 | 191.4 |
| savanna | 2.224 | 0.001 | 1845962 | 3249381 | 192.6 |
| grassland | 2.154 | 0.003 | 122984 | 232839 | 183.1 |
| cropland | 2.491 | 0.004 | 132668 | 306182 | 133.8 |
| wetland | 2.008 | 0.023 | 1902 | 2626 | 130.9 |

Cerrado bolgesi tek basina: alpha = 2.086 +- 0.002 (N = 208313); literatur 1.31.

## Hold-out (2020), 30735 hucre-gun

Sapma = toplam(MODIS x R_rel x k) / toplam(VIIRS) - 1. MedAE = medyan |ln(tahmin/olculen)| (hucre-gun basina). Biyom tablosunda hicbir seviyede satir yoksa enlem bandi kullanildi (15 hucre-gun).

| Kapsam | N | sapma enlem | sapma biyom | sapma k=1 | MedAE enlem | MedAE biyom | MedAE k=1 |
|---|---:|---:|---:|---:|---:|---:|---:|
| Hepsi | 30735 | -3.8% | +1.9% | -14.2% | 0.476 | 0.458 | 0.508 |
| forest | 7432 | +7.1% | +6.8% | -5.3% | 0.490 | 0.466 | 0.520 |
| shrubland | 1024 | -3.3% | +8.6% | -14.7% | 0.533 | 0.510 | 0.581 |
| savanna | 16026 | -8.1% | -0.1% | -17.2% | 0.438 | 0.423 | 0.476 |
| grassland | 2099 | -12.1% | -6.4% | -22.3% | 0.516 | 0.499 | 0.562 |
| cropland | 4139 | -5.7% | +3.6% | -19.9% | 0.569 | 0.567 | 0.598 |
| wetland | 14 | +16.2% | +16.2% | +4.7% | 0.238 | 0.238 | 0.283 |
| tundra | 1 | +19.4% | +19.4% | -6.7% | 0.177 | 0.177 | 0.070 |

Sonuc: BIYOM TABLOSU ENLEM BANDLARINDAN IYI (hem medyan |log hata| hem toplam sapma).
