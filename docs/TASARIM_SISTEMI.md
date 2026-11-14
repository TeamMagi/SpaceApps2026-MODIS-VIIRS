# PyroSync Tasarım Sistemi

Bu doküman, `web/src/styles/tokens.css` ve `web/src/styles/global.css` dosyalarından üretilmiş tek doğruluk kaynağıdır.

## 1. Kullanım İlkeleri

- **Sıfır sabit değer:** Component CSS dosyalarında sabit hex/rgb renk, sabit `px`/`rem` boşluk, font boyutu, radius veya `z-index` yazılmaz; her zaman `tokens.css` içindeki CSS değişkenleri kullanılır.
- **CSS Modules zorunluluğu:** `global.css` içinde yalnızca reset kuralları ile `.sr-only` ve `.tabular` yardımcı sınıfları bulunur. Tüm bileşen stilleri CSS Modules içinde yazılır.
- **Doygunluk bütçesi:** Arayüz kromundaki tek doygun renk `--color-accent` değişkenidir; yalnızca Raw/Harmonized geçişi ve 2012 işaretçisi için ayrılmıştır. Sıcak tonlar FRP veri skalasına ait olduğundan aksan camgöbeği (cyan) seçilmiştir.
- **Erişilebilirlik:** `--color-text-disabled` hariç tüm metin token'ları `--color-bg` üzerinde en az 4.5:1 kontrast sağlar. Odak halkası `:focus-visible` ile `2px solid var(--color-focus)` (`outline-offset: 2px`, `border-radius: var(--radius-sm)`) olarak uygulanır. Sayısal veriler `body` düzeyinde `tabular-nums` alır.

## 2. Renk Sistemi

Varsayılan tema **dark** (projektör öncelikli) olup light tema `[data-theme="light"]` ile etkinleştirilir.

### Yüzey, Kenar, Metin ve Aksan

| Kategori | Token | Dark (Varsayılan) | Light |
| --- | --- | --- | --- |
| Yüzey | `--color-bg` / `--color-bg-outer` | `#0d0f13` / `#07080a` | `#f6f7f9` / `#eceef2` |
| Yüzey | `--color-surface` / `--color-surface-raised` | `#14171d` / `#1b1f27` | `#ffffff` / `#ffffff` |
| Yüzey | `--color-surface-sunken` | `#0a0c0f` | `#eef0f3` |
| Etkileşim | `--color-hover` / `--color-active` | `#232833` / `#2c3240` | `#eceef2` / `#e1e4ea` |
| Perde | `--color-scrim` | `rgb(5 6 8 / 0.62)` | `rgb(20 24 31 / 0.38)` |
| Kenar | `--color-border-subtle` / `--color-border` / `--color-border-strong` | `#222731` / `#313846` / `#495266` | `#e3e6eb` / `#cfd4dc` / `#9aa2b1` |
| Odak | `--color-focus` | `#d0d6e2` | `#1b2230` |
| Metin | `--color-text` / `--color-text-secondary` | `#f1f3f7` / `#a9b1c0` | `#12161d` / `#424b5a` |
| Metin | `--color-text-muted` / `--color-text-disabled` / `--color-text-inverse` | `#858d9c` / `#4f5665` / `#0d0f13` | `#5d6676` / `#a6adba` / `#ffffff` |
| Aksan | `--color-accent` / `--color-accent-strong` | `#3cc8f0` / `#7ad9f5` | `#0a6f99` / `#075679` |
| Aksan | `--color-accent-dim` / `--color-accent-contrast` | `rgb(60 200 240 / 0.16)` / `#061218` | `rgb(10 111 153 / 0.12)` / `#ffffff` |
| Durum | `--color-status-live` / `--color-status-demo` / `--color-status-error` | `#9fd8b0` / `#c9b98f` / `#e3a99b` | `#2f7d4a` / `#8a6d1f` / `#a9452f` |

> Durum token'ları yalnızca rozet noktası (badge dot) içindir ve her zaman açıklayıcı metinle birlikte kullanılır.

### Sensörler (MODIS / VIIRS / Harmonized) ve Harita Paleti

Raw modda sensörler renk yerine **şekil ayrımıyla** (renk körlüğü güvenliği) ayırt edilir: **MODIS içi boş daire**, **VIIRS dolu kare**. Harmonized modda ayrı bir sensör renk token'ı yoktur; geçiş ve aktiflik `--color-accent`, yangın şiddeti ise FRP/anomali skalalarıyla gösterilir.

| Kullanım | Token | Dark (Varsayılan) | Light |
| --- | --- | --- | --- |
| Sensör (UI) | `--color-sensor-modis` / `--color-sensor-viirs` / `--color-duplicate-ring` | `#d4d9e2` / `#8d96a6` / `#f1f3f7` | `#3a4252` / `#8a93a3` / `#12161d` |
| Sensör (Harita) | `--map-modis` / `--map-viirs` / `--map-duplicate-ring` | `#e2e7f0` / `#96a1b5` / `#f1f3f7` | `#2d3545` / `#687385` / `#12161d` |
| Harita zemin | `--map-water` / `--map-land` / `--map-land-selected` / `--map-land-dim` | `#070a0f` / `#151922` / `#202736` / `#10141c` | `#dfe5ee` / `#f3f5f8` / `#ffffff` / `#eaedf2` |
| Harita çizgi | `--map-border` / `--map-border-selected` / `--map-ink` / `--map-halo-light` / `--map-focus` | `#283040` / `#7b88a1` / `#f1f3f7` / `#0a0d12` / `#d0d6e2` | `#c5cdd8` / `#4a5568` / `#12161d` / `#ffffff` / `#1b2230` |

### FRP (Sıralı, Düşük -> Yüksek) ve Anomali (PuOr) Skalaları

Dark temada sıcak yangınlar parlak (`--frp-8` açık sarı), light temada ise kontrast için koyu (`--frp-8` koyu mor) uca gider. Anomali skalasında mor normalin altı, turuncu normalin üstüdür.

| FRP Token | Dark | Light | Anomali Token | Dark | Light |
| --- | --- | --- | --- | --- | --- |
| `--frp-1` | `#3d0f5e` | `#fbe7a1` | `--anom-neg-3` | `#6a51a3` | `#542788` |
| `--frp-2` | `#6a176e` | `#fcc85a` | `--anom-neg-2` | `#8c7cc0` | `#8073ac` |
| `--frp-3` | `#932667` | `#f89b2c` | `--anom-neg-1` | `#b8afd8` | `#c7c2e0` |
| `--frp-4` | `#bc3754` | `#e8682a` | `--anom-0` | `#2a2f3a` | `#eceef2` |
| `--frp-5` | `#dd513a` | `#c8413f` | `--anom-pos-1` | `#f6c27a` | `#fdc983` |
| `--frp-6` | `#f37819` | `#9c2a5c` | `--anom-pos-2` | `#e68a2a` | `#e08214` |
| `--frp-7` | `#fcb519` | `#6e1c6c` | `--anom-pos-3` | `#b85a0d` | `#a14e05` |
| `--frp-8` | `#f6e27a` | `#3d0f5e` | `--anom-none` | `#15181e` | `#f9fafb` |

## 3. Tipografi

| Kategori | Token | Değer |
| --- | --- | --- |
| Font ailesi | `--font-sans` | `"Inter", system-ui, -apple-system, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif` |
| Font ailesi | `--font-mono` | `"JetBrains Mono", ui-monospace, SFMono-Regular, Menlo, Consolas, "Liberation Mono", monospace` |
| Boyut (küçük) | `--font-size-2xs` / `--font-size-xs` / `--font-size-sm` | `0.75rem` (12px) / `0.78125rem` (12.5px) / `0.8125rem` (13px) |
| Boyut (gövde/başlık) | `--font-size-md` / `--font-size-base` / `--font-size-lg` | `0.875rem` (14px, `body` varsayılanı) / `1rem` (16px) / `1.125rem` (18px) |
| Boyut (büyük) | `--font-size-xl` / `--font-size-2xl` / `--font-size-display` | `1.375rem` (22px) / `1.75rem` (28px) / `2.25rem` (36px) |
| Satır yüksekliği | `--line-height-tight` / `--line-height-snug` / `--line-height-body` | `1.2` / `1.35` / `1.5` |
| Ağırlık | `--font-weight-regular` / `--font-weight-medium` / `--font-weight-semibold` / `--font-weight-bold` | `400` / `500` / `600` / `700` |
| Harf aralığı | `--letter-spacing-caps` | `0.06em` |

## 4. Spacing, Radius, Gölge, Layout, Motion ve Z-Index

**Spacing (8px grid + yarım adım):** `--space-0: 0`, `--space-half: 4px`, `--space-1: 8px`, `--space-2: 16px`, `--space-3: 24px`, `--space-4: 32px`, `--space-5: 40px`, `--space-6: 48px`, `--space-8: 64px`.

**Radius & Kenarlık:** `--radius-sm: 4px`, `--radius-md: 6px`, `--radius-lg: 8px`, `--radius-xl: 12px`, `--radius-pill: 999px`, `--border-width: 1px`.

**Gölgeler:**
- `--shadow-raised`: Dark `0 1px 2px rgb(0 0 0 / 0.4), 0 8px 24px rgb(0 0 0 / 0.35)` | Light `0 1px 2px rgb(18 22 29 / 0.08), 0 8px 24px rgb(18 22 29 / 0.1)`
- `--shadow-drawer`: Dark `-12px 0 40px rgb(0 0 0 / 0.5)` | Light `-12px 0 40px rgb(18 22 29 / 0.16)`

**Layout (masaüstü referansı 1366x768):** `--h-header: 48px`, `--h-command: 100px`, `--h-pipeline: 42px`, `--h-chart: 220px`, `--h-calendar: 260px`, `--h-bottom: 48px`, `--w-map: 58%`, `--w-drawer: 420px`, `--toggle-w: 260px`, `--toggle-h: 48px`, `--h-map-mobile: clamp(360px, 60dvh, 640px)`, `--calendar-cell-mobile: 26px`.

**Z-Index katmanları:** `--z-map-overlay: 10`, `--z-sticky: 20`, `--z-toggle-dock: 30`, `--z-popover: 40`, `--z-drawer: 50`, `--z-tooltip: 60`.

**Motion ve Reduced Motion:**
- Süreler: `--dur-detect: 400ms`, `--dur-merge: 600ms`, `--dur-settle: 400ms`, `--dur-full: 1400ms`, `--dur-short: 500ms`, `--dur-reduced: 150ms`, `--dur-ui: 150ms`, `--dur-drawer: 240ms`.
- Akış saat değişkenleri (CSS Modules `transition-duration: var(--run-merge, var(--dur-merge))` biçiminde kullanır): `--run-detect`, `--run-merge`, `--run-settle`.
- Eğriler: `--ease-standard: cubic-bezier(0.2, 0, 0, 1)`, `--ease-emphasized: cubic-bezier(0.3, 0, 0, 1)`, `--ease-exit: cubic-bezier(0.3, 0, 1, 1)`.
- `@media (prefers-reduced-motion: reduce)` altında `--dur-detect` ve `--dur-settle` `0ms` olur; diğer `--dur-*` token'ları `--dur-reduced` (`150ms`) değerine düşer.

## 5. Breakpoint ve Medya Sorguları

`global.css` ve `tokens.css` içinde genişlik tabanlı (`min-width` / `max-width`) breakpoint tanımlı değildir. Yalnızca şu medya sorguları ve boyut ipuçları vardır:
- Masaüstü layout referansı `1366x768` olarak belgelenmiştir; mobil görünüm için `--h-map-mobile` ve `--calendar-cell-mobile` token'ları tanımlıdır.
- `@media (prefers-reduced-motion: reduce)`: Süre token'larını kısaltır, `html { scroll-behavior: auto; }` ve `animation-iteration-count: 1 !important;` uygular.
- `@media print`: `body` arka planını `#fff`, metni `#000` yapar.

## 6. Bileşen Kalıpları

| Bileşen | Yüzey / Arka Plan | Kenarlık / Gölge | Metin / Etkileşim / Katman |
| --- | --- | --- | --- |
| **Button** | Varsayılan `transparent` (`global.css` reset) veya `--color-surface`; birincil aksiyon (toggle/2012) `--color-accent` + `--color-accent-contrast` | `--border-width solid var(--color-border)`, `--radius-md` (`--radius-pill` toggle için) | `--color-text`, hover `--color-hover`, active `--color-active`, disabled `--color-text-disabled`, odak `--color-focus`, süre `--dur-ui` + `--ease-standard` |
| **Card / Panel** | `--color-surface` (yükseltilmiş ise `--color-surface-raised`, gömülü ise `--color-surface-sunken`) | `--border-width solid var(--color-border-subtle)` veya `--color-border`, `--radius-lg` / `--radius-xl`, opsiyonel `--shadow-raised` | İç boşluk `--space-2` / `--space-3`, başlık `--color-text`, alt metin `--color-text-secondary` / `--color-text-muted` |
| **Tooltip** | `--color-surface-raised` | `--border-width solid var(--color-border-strong)`, `--radius-sm` / `--radius-md`, `--shadow-raised` | `--color-text`, `--font-size-xs` / `--font-size-sm`, iç boşluk `--space-half` / `--space-1`, katman `--z-tooltip`, süre `--dur-ui` |
| **Drawer** | `--color-surface` (arka perde `--color-scrim`) | Sol kenar `--border-width solid var(--color-border)`, `--shadow-drawer`, genişlik `--w-drawer` | İç boşluk `--space-3`, katman `--z-drawer`, açılma süresi `--dur-drawer` + `--ease-emphasized` / `--ease-exit` |

## 7. Tema Değiştirme

- Varsayılan tema `:root` ve `:root[data-theme="dark"]` seçicileriyle aktifleşir (`color-scheme: dark`).
- Light tema `document.documentElement.setAttribute("data-theme", "light")` ile `:root[data-theme="light"]` üzerinden uygulanır (`color-scheme: light`).
- Tüm renkler ve gölgeler CSS değişkeni olduğu için bileşenlerde tema bazlı ek CSS sınıfı yazılmaz.

## 8. Yapılmaması Gerekenler

- CSS Modules içinde sabit `#hex`, `rgb(...)`, `px` boşluk, `z-index` sayısı veya ham `ms` süresi tanımlama.
- `--color-accent` rengini Raw/Harmonized geçişi ve 2012 işaretçisi dışındaki düğme, link veya dekoratif alanlarda kullanma.
- MODIS ve VIIRS sensörlerini yalnızca renkle ayırt etme (şekil farkı korunmalı: MODIS içi boş daire, VIIRS dolu kare).
- Durum renklerini (`--color-status-live`, `--color-status-demo`, `--color-status-error`) açıklayıcı metin olmadan tek başına veya arka plan dolgusu olarak kullanma.
- `global.css` içine bileşene özgü global sınıf ekleme veya `* { transition-duration: ... }` gibi genel geçiş zorlaması yazma (reduced-motion token'lar üzerinden yönetilir).
