# web — React Frontend

## Kesin Kurallar
- Commit mesajlarına `Co-Authored-By` veya AI imzası eklenmez.
- TypeScript derleyicisi **SWC** olmalıdır (`@vitejs/plugin-react-swc`). Babel kullanılmaz.
- Strict TypeScript: `"strict": true`, `"noUnusedLocals": true`, `"noUnusedParameters": true`.
- Commit öncesi `npm run typecheck && npm test && npm run build` çalıştırılır (`make test` frontend için yalnızca `npm run build` çalıştırır; tip hatalarını ve testleri yakalamaz).

## Dizin Yapısı

```
public/
├── geo/            → Üretilen regions.json (commit'lenir) ve SOURCE.md
└── globe/          → 3D küre dünya dokusu ve tile'lar (commit'lenir)
scripts/
└── build-geo.mjs   → public/geo/regions.json üretim betiği
tests/
├── *.test.ts       → node:test birim testleri
└── fixtures/       → Paylaşılan test fixture'ları
src/
├── components/     → UI bileşenleri (her bileşen kendi klasöründe)
│   ├── ErrorBoundary/ → Hata sınırı
│   ├── Layout/     → Sayfa düzeni (AppShell)
│   ├── Header/     → Üst çubuk, bölge seçici, dil ve mod rozeti
│   ├── Headline/   → Soru / cevap başlığı
│   ├── Toggle/     → Raw ↔ Harmonized geçiş düğmesi
│   ├── Stats/      → Sayı farkı ve düzeltme fişi
│   ├── Pipeline/   → 7 aşamalı harmonizasyon şeridi
│   ├── Map/        → HotspotMap, FlatMap, GlobeMap, FootprintLens, PointDetail, MapLegend,
│   │                 engine/ (2D canvas: renderer, viewport, LOD, point index, hit testing),
│   │                 globe/ (three.js 3D küre: scene, tiles, textures, picking)
│   ├── Charts/     → Uçurum grafiği (elle yazılmış SVG)
│   ├── Calendar/   → Radyal yanma takvimi (BurnCalendar) ve tooltip
│   ├── BottomBar/  → Alt şerit (model uyarısı, CSV, brifing)
│   └── Drawers/    → Sağdan açılan çekmeceler (Methods, Briefing)
├── content/
│   ├── copy.ts     → Ana sözlük (EN + TR, tipli), t(), createTranslator
│   └── copyMap.ts, copyRegion.ts, copyState.ts → copy.ts içine birleştirilen parçalar
├── hooks/          → Custom React hooks (useHarmonizeTransition, useCopy, vb.)
├── lib/
│   ├── geo/        → firmsCountries.json, loadGeo.ts, projection.ts, regionMatch.ts
│   └── ...         → Diğer yardımcılar (urlState, formatlama vb.)
├── services/
│   └── api.ts      → Rust backend API katmanı (/api → localhost:3001; mock fallback yoktur)
├── store/
│   └── useAppStore.ts → zustand uygulama durumu
├── styles/         → tokens.css + global.css
├── types/
│   └── index.ts    → Paylaşılan TypeScript tipleri (backend ile senkron)
├── App.tsx         → Ana uygulama bileşeni
├── main.tsx        → React root
└── vite-env.d.ts   → Vite ortam tipleri
```

## Bileşen ve Veri Kuralları

- Her bileşen kendi alt dizininde bulunur: `components/FeatureName/FeatureName.tsx`.
- Props interface'leri bileşen dosyasında tanımlanır; paylaşılan tipler `types/index.ts`'te tutulur.
- API çağrıları bileşenlerde yapılmaz, `services/api.ts` kullanılır. Mock verisi yoktur; `make dev-backend` çalışmıyorsa UI hata/boş durumlarını gösterir.
- State yönetimi için custom hook'lar `hooks/` dizininde oluşturulur.
- Görünür tüm metinler ve `aria-label` değerleri `content/copy.ts` üzerinden gelir; her anahtar hem `en` hem `tr` içermeli ve parça dosyalar (`copyMap.ts`, `copyRegion.ts`, `copyState.ts`) `copy.ts` anahtarlarıyla çakışmamalıdır.
- Tüm animasyon zamanlaması `useHarmonizeTransition` (tek saat) üzerinden okunur; bileşenler kendi geçiş zamanlayıcısı kurmaz.
- Raw modu, `revealAnswer` true olmadan harmonize cevabı (sayı, başlık veya fiş) asla sızdırmaz.

## Geliştirme ve Test

- `npm run dev` → Vite dev server (port 5173), `/api` istekleri `localhost:3001`'e proxy edilir.
- `npm run build` → `vite build` (`dist/` dizinine). **Önemli:** SWC tipleri soyup atar, type-check yapmaz; başarılı bir build tiplerin doğru olduğu anlamına gelmez.
- `npm run typecheck` → `tsc --noEmit` (yalnızca `src/` kapsamındadır; `tests/` tsconfig `include` dışındadır).
- `npm test` → esbuild `tests/*.test.ts` dosyalarını `.test-out/` içine (gitignore'da) paketler ve `node --test` çalıştırır. **Node 22+ gerektirir** (`node --test ".test-out/*.mjs"` glob'u Node 20'de desteklenmez; Docker imajı Node 20 kullanır ancak yalnızca `npm run build` çalıştırdığı için sorun olmaz).
- `npm run geo:build` → `world-atlas`, `topojson-client` (devDependencies) ve `src/lib/geo/firmsCountries.json` üzerinden `public/geo/regions.json` dosyasını yeniden üretir (çıktı sınırı 700 KB). Yalnızca ülke tablosu veya sadeleştirme değiştiğinde çalıştırılır ve üretilen dosya commit'lenir.
- Yeni testler `tests/<ad>.test.ts` altında `node:test` + `node:assert` ile yazılır, `../src/...` göreceli yollarıyla import edilir; paylaşılan fixture'lar `tests/fixtures/` içine konur.

## Stil

- CSS Modules (`*.module.css`) ve `styles/tokens.css` içindeki CSS custom properties (tasarım token'ları) kullanılır.
- `style={{...}}` yalnızca çalışma zamanında hesaplanan değerler için (tooltip konumu, motion değerleri) kullanılabilir; statik stiller veya renkler için kesinlikle kullanılmaz.
- Responsive tasarım: mobile-first `min-width` kırılım noktaları (`640px`, `1024px`); `@media (pointer: coarse)` dokunma hedefleri, `@media (prefers-reduced-motion: reduce)` ve brifing için print stilleri desteklenir.

## Kütüphaneler

- Mevcut kütüphaneler: `three` (yalnızca 3D küre), `motion` (animasyon), `zustand` (durum yönetimi).
- Grafikler `components/Charts/` altında elle yazılmış SVG olarak çizilir; yeni bir grafik, harita veya UI kütüphanesi eklemek için ekip kararı gerekir.
- Yeni dependency eklerken SWC uyumluluğu kontrol edilir; `three` ve `@types/three` sürümleri birebir aynı sürüme sabitlenir.
