# PyroSync — İlk Kurulum

Bu belge, projeyi ilk kez kendi bilgisayarında çalıştırmak isteyen ekip üyeleri içindir. Adımlar sırayla uygulandığında sunucu ve arayüz Docker üzerinde `http://localhost:3001` adresinde çalışır.

## Kısa yol

```bash
git clone https://github.com/TeamMagi/SpaceApps2026-MODIS-VIIRS.git && cd SpaceApps2026-MODIS-VIIRS
cp .env.example .env        # .env içinde FIRMS_API_KEY değerini yaz
make data                   # bir kez, ~9 GB indirme, ~41 GB disk
make verify-data            # "Archive verification OK (10095/10095 files)" görmelisin
make docker-up              # http://localhost:3001
```

## 1. Ön koşullar

- **Git**
- **Git LFS**: küre dokuları (`web/public/globe/`, 129 dosya) Git LFS ile tutuluyor; Git for Windows çoğunlukla içerir. Kontrol: `git lfs version`; yoksa `git lfs install` (kurulum: https://git-lfs.com). LFS olmadan klonlanırsa dokular birkaç satırlık işaretçi dosyası olarak gelir; düzeltmek için `git lfs install && git lfs pull`.
- **Docker Desktop**: kurulu ve çalışır durumda olmalı.
- **make**: Windows'ta varsayılan gelmez. Bir kez kurmak için:

```bash
winget install ezwinports.make
```

Kurulumdan sonra terminali yeniden aç. Bu projedeki tüm komutlar **Git Bash** içinden çalıştırılır.

- **Rust ve Node.js** yalnızca Docker'sız yerel geliştirme için gerekir. Bu kurulum adımları için gerekmez.

## 2. Kodu al

İlk kez:

```bash
git clone https://github.com/TeamMagi/SpaceApps2026-MODIS-VIIRS.git
cd SpaceApps2026-MODIS-VIIRS
```

Mevcut kopyada:

```bash
git pull
```

## 3. `.env` dosyasını hazırla

```bash
cp .env.example .env
```

Ardından `.env` içinde `FIRMS_API_KEY=` satırına anahtarını yaz.

- Anahtarı ücretsiz al: https://firms.modaps.eosdis.nasa.gov/api/map_key/
- Anahtar, canlı (NRT) FIRMS endpoint'leri için gereklidir. Anahtar yoksa sunucu açılır ama bu endpoint'ler 503 döner.
- Tarihsel arşiv için anahtar **gerekmez**.
- `.env` dosyası **asla commit edilmez**. Şablon olan `.env.example` ise repoda kalır.

**Yalnızca Linux:** `.env` içine `DOCKER_USER` değerini gerçek sayılarla yaz, örneğin `DOCKER_USER=1000:1000`. Bu sayılar `id -u` ve `id -g` komutlarının çıktısıdır. `DOCKER_USER=$(id -u):$(id -g)` satırını `.env` içine olduğu gibi yazma: Docker Compose `.env` dosyasında komut çalıştırmaz, bu değer çalışmaz. Windows ve macOS'ta bu ayar gerekmez.

## 4. Tarihsel arşivi indir (`make data`)

```bash
make data
```

- Tüm FIRMS ülke arşivini indirir: yaklaşık **9 GB** indirme, diskte yaklaşık **41 GB**.
- Kapsam: 238 ülke/bölge; MODIS 2000–2025, VIIRS SNPP 2012–2025, VIIRS NOAA-20 2018–2025.
- VIIRS NOAA-21 arşivde yoktur, yalnızca canlı veride bulunur.
- Anahtar gerektirmez ve FIRMS işlem hakkı tüketmez.
- Bir kez yapılır ve uzun sürer. Ctrl+C ile kesilebilir; tekrar `make data` çalıştırmak kaldığı yerden devam eder.
- Bu adım yalnızca `/api/calendar`, `/api/regions`, `/api/series` ve `/api/points` için gereklidir. Yalnızca canlı endpoint'lerle çalışacaksan atlayabilirsin.

**`make precompute` gerekmez.** 238 bölgenin önceden hesaplanmış `/api/series` sonuçları repoda `data/derived/` altında gelir; büyük ülkeler de hemen açılır. `make precompute` yalnızca harmonizasyon ayarları (`pyrosync.toml` ya da `PYROSYNC_*` değişkenleri) değiştiğinde gerekir.

## 5. Bütünlüğü doğrula (`make verify-data`)

```bash
make verify-data
```

Her dosya, repodaki SHA-256 manifest'i (`data/archive-manifest.sha256`, 10095 dosya) ile karşılaştırılır. Beklenen çıktı:

```text
Archive verification OK (10095/10095 files)
```

Bu doğrulama, ekibin herkesin bit düzeyinde aynı veriye sahip olduğunu gösterir. Hata alırsan [Sorun giderme](#sorun-giderme) bölümüne bak.

## 6. Sunucuyu başlat (`make docker-up`)

```bash
make docker-up
```

- Arayüz: http://localhost:3001
- Sağlık kontrolü: http://localhost:3001/api/health
- Logları izlemek için: `make docker-logs`

Arşiv indirilmemişse komut bir uyarı yazar; canlı endpoint'ler yine de çalışır.

Kurulumu doğrulamak için:

```bash
make smoke
```

Tüm kontroller `PASS` olmalıdır.

## Güncelleme

Kod değiştiğinde (sonraki günlerde):

```bash
git pull
make docker
make docker-up
```

- Arşiv yeniden indirilmez.
- `data/archive-manifest.sha256` dosyası değiştiyse `make verify-data` komutunu tekrar çalıştır.

## Sorun giderme

- **Arşiv endpoint'leri 404 ya da "make data" uyarısı veriyor:** Arşiv eksik ya da indirilmemiş. `make data` çalıştır.
- **`make verify-data` hata veriyor:** Eksik dosyalar olabilir. `make data` komutunu tekrar çalıştır; eksikleri tamamlar.
- **`make docker-up` başarısız oluyor:** Docker Desktop çalışmıyor olabilir. Docker Desktop'ı başlatıp tekrar dene.
- **`.env` bulunamadı hatası:** `cp .env.example .env` adımını yapmadın.
- **Port 3001 dolu:** `.env` içinde `PORT` değerini değiştir. `docker-compose.yml` hem host hem container portunu bu değişkenden alır; adres de buna göre değişir (örneğin `http://localhost:3002`).
- **Canlı endpoint'ler 503 dönüyor:** `FIRMS_API_KEY` eksik ya da yanlış. `.env` dosyasını kontrol et, sonra `make docker-down` ve `make docker-up` çalıştır.
- **Disk dolu uyarısı:** Arşiv yaklaşık 41 GB, Docker imajları da ayrıca yer kaplar.
- **`make: command not found`:** `winget install ezwinports.make` kurulumunu yap, terminali yeniden aç ve Git Bash kullan.

## Faydalı komutlar

| Komut | Açıklama |
|-------|----------|
| `make docker-up` | Konteynerleri başlatır (`.env` gerektirir) |
| `make docker-down` | Konteynerleri durdurur |
| `make docker-logs` | Konteyner loglarını izler |
| `make data` | Tam FIRMS ülke arşivini indirir (~41 GB, anahtar gerekmez) |
| `make verify-data` | Yerel arşivi `data/archive-manifest.sha256` ile doğrular |
| `make precompute` | `/api/series` önbelleğini yeniden hesaplar (yalnızca ayar değişince) |
| `make bundle-derived` | Hesaplanan `/api/series` sonuçlarını `data/derived/series` altına kopyalar (bakımcı) |
| `make test` | Tüm testleri çalıştırır (yerel Rust ve Node gerektirir) |
| `make smoke` | Çalışan sunucuya karşı uçtan uca duman testlerini çalıştırır |
| `make help` | Tüm komutları listeler |
