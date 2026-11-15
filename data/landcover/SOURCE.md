# Arazi Örtüsü / Biyom Izgarası — Kaynak ve Lisans Notu

`biome_0p25.rle`, K14 (biyom bazlı tamlık katsayısı) için küresel 0.25° biyom katmanıdır. Dosya
`crates/core/src/biome.rs` içine `include_bytes!` ile gömülür; çalışma anında dosya yolu, ayar veya I/O yoktur.

## Kaynak
- **Veri seti:** WRF/WPS statik coğrafya verisi `modis_landuse_20class_5m_with_lakes` (NCAR/MMM).
- **URL:** <https://www2.mmm.ucar.edu/wrf/src/wps_files/modis_landuse_20class_5m_with_lakes.tar.bz2>
- **İndirme tarihi:** 2026-11-15 (oturum açmadan indirilebilir; tar.bz2, 8036926 bayt)
- **Tarball SHA-256:** `d8f7dfa9b45f86ba46f96aee6e0202209b67043e4b9b782e39a59f9d076468e0`
- **Çıktı SHA-256:** `def1d4a0f07f770e640a00195c2d81fdf731011a9cd1fed6784c85b99f8721fd` (`biome_0p25.rle`, 126276 bayt)
- **İçerik:** MODIS IGBP arazi örtüsü, 20 sınıf + göl sınıfı (Noah-modified 21 kategori, `mminlu="MODIFIED_IGBP_MODIS_NOAH"`),
  5 yay dakikası (4320 × 2160, `dx = dy = 0.0833333`), geogrid ikili döşemeleri (600 × 600, 3 hücre sınır, 21 katman,
  `wordsize = 1`). Her katman bir kategorinin hücredeki yüzde payıdır; satır sırası güneyden kuzeye (`known_lat = -89.9583`).
- **`index` dosyasındaki açıklama (birebir):**
  `description="Noah-modified 21-category IGBP-MODIS landuse, subsampled by NCAR/MMM 2018-05-23 from 30-arc-second"`
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
