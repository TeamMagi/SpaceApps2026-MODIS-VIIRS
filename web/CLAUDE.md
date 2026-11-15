# web — React Frontend

## Kesin Kurallar
- Commit mesajlarına `Co-Authored-By` veya AI imzası eklenmez.
- TypeScript derleyicisi **SWC** olmalıdır (`@vitejs/plugin-react-swc`). Babel kullanılmaz.
- Strict TypeScript: `"strict": true`, `"noUnusedLocals": true`, `"noUnusedParameters": true`.
- Commit öncesi `npm run typecheck && npm test && npm run build` çalıştırılır (`make test` frontend için yalnızca `npm run build` çalıştırır; tip hatalarını ve testleri yakalamaz).

## Dizin Yapısı

```
mock.html           → Sentetik önizleme girişi (src/mocks/preview.tsx)
public/
├── geo/            → Üretilen regions.json (commit'lenir) ve SOURCE.md
└── globe/          → 3D küre dünya dokusu ve tile'lar (commit'lenir)
scripts/
├── build-geo.mjs      → public/geo/regions.json üretim betiği
└── generate-mocks.mjs → src/mocks/ sentetik JSON fixture üretim betiği
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
│   ├── Stats/      → Sayı farkı ve düzeltme fişi (Receipt, StatsCards)
│   ├── Pipeline/   → 7 aşamalı harmonizasyon şeridi
│   ├── Map/        → HotspotMap, FlatMap, GlobeMap, FootprintLens, PointDetail, MapLegend,
│   │                 engine/ (2D canvas: renderer, viewport, LOD, point index, hit testing),
│   │                 globe/ (three.js 3D küre: scene, tiles, textures, picking)
│   ├── Charts/     → ChartPanel (Tespitler / Ortalama FRP / Sensörler sekmeleri), CliffChart, FrpBandChart, SensorCharts (Y2: aylık sensör çubukları, eşleşen çift FRP serpilmesi, kaynak halkası; veri şekillendirme `lib/sensorCharts.ts`) ve SensorComparison (yalnızca mock önizleme; elle yazılmış SVG)
│   ├── Calendar/   → Radyal yanma takvimi (BurnCalendar) ve tooltip; "Ayrıntılar" düğmesi ve `D` tuşu hücre tanı çekmecesini açar (ay filtresi tıklamayla aynı kalır)
│   ├── OrbitalDrift/ → OrbitalDriftStrip (Y9): `modis_orbital_drift` bayrağı veya Terra/Aqua geçiş saati medyanı nominalden sapınca görünen, oturum boyunca kapatılabilir bilgi şeridi (`role="note"`)
│   ├── BottomBar/  → Alt şerit (model uyarısı, CSV, brifing)
│   ├── Tour/       → 60 saniyelik rehberli tur kartı (GuidedTour; zamanlayıcılar hooks/useGuidedTour'da)
│   ├── Drawers/    → Sağdan açılan çekmeceler (Methods, Briefing, CellDiagnosticDrawer: hücre tanı çekmecesi, Y9)
│   └── Methods/    → Tam Methods sayfası (#/methods: MethodsPage, MethodsPipeline akordeon, VzaCurveFigure, H3GridFigure, MethodsReferences, DisclaimerBanner, MethodsLink, useMethodsConfig)
├── content/
│   ├── copy.ts     → Ana sözlük (EN + TR, tipli), t(), createTranslator
│   ├── copyMap.ts, copyRegion.ts, copyState.ts, copyMethods.ts, copyAnalytics.ts, copyTour.ts, copyFusion.ts, copyBriefing.ts, copyDiagnostics.ts (hücre tanı çekmecesi + Orbital Drift şeridi) → copy.ts parçaları
│   └── references.ts → Methods sayfası kaynakça listesi
├── hooks/          → Custom React hooks (useHarmonizeTransition, useCopy, useRoute, vb.)
├── lib/
│   ├── geo/        → firmsCountries.json, loadGeo.ts, projection.ts, regionMatch.ts
│   ├── route.ts    → Hash router (#/methods[/section], METHODS_SECTIONS)
│   └── ...         → Diğer yardımcılar (urlState, formatlama vb.)
├── mocks/          → Sentetik geliştirme fixture'ları (Y5): mock.html önizlemesi ve demo modu (K1, `lib/demoData.ts` üzerinden)
├── services/
│   ├── api.ts            → Rust backend API katmanı (/api → localhost:3001; backend'e erişilemezse demo moduna düşer, K1)
│   └── pipelineConfig.ts → Methods sayfası için GET /api/config tipleri ve fetch
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
- API çağrıları bileşenlerde yapılmaz, `services/api.ts` kullanılır. Ana uygulama veriyi backend'den okur; backend'e erişilemezse (ağ hatası, zaman aşımı, `/api/health` 5xx) demo moduna (K1) düşer ve `src/mocks/` fixture'larını YALNIZCA `lib/demoData.ts` üzerinden okur. Demo modunda her kaynak etiketi ve CSV çıktısı verinin sentetik olduğunu söyler; fixture'lar hiçbir yerde NASA verisi gibi sunulmaz. Başka hiçbir modül `src/mocks/`'tan import etmez. HTTP 4xx yanıtları gerçek yanıttır ve demo'ya düşürmez.
- `src/mocks/` sentetik geliştirme fixture'larını içerir (Y5: `hotspots.json`, `calendar.json`, `stats.json`, `raw-platforms.json`, `metadata.json`, `index.ts`, `preview.tsx`, `README.md`; NASA verisi veya pipeline çıktısı değildir). `npm run dev` altında `http://localhost:5173/mock.html` (`web/mock.html` → `src/mocks/preview.tsx`) ayrı önizleme girişi ve ana uygulamanın demo modu (`lib/demoData.ts`) tarafından kullanılır. JSON fixture'lar elle düzenlenmez; repo kökünden deterministik seed ile `node web/scripts/generate-mocks.mjs` ile yeniden üretilir (ayrıntılar `src/mocks/README.md`).
- **Dışa aktarma (R7):** `hooks/useExportView` canlı modda `GET /api/export` (`fetchExport`), demo modunda istemcide `lib/csv.ts` / `lib/geojson.ts` ile dosya üretir. Türkiye vaka çalışması (S7) `lib/caseStudy.ts` içinde `data/case-study/turkey/stats.json` dosyasını (web/ dışında) dinamik `import()` ile yükler; `vite.config.ts` `server.fs.allow` ve Dockerfile'daki COPY bu yüzden vardır. Brifing metrikleri `lib/briefingMetrics.ts` içinde saf fonksiyonlardır.
- **Hücre tanıları ve Orbital Drift (Y9):** Saf yardımcılar `lib/cellDiagnostic.ts` (Raw modunda harmonize türevi değer döndürmeyen `buildCellDiagnostic`, `quality_flags` etiket tablosu, Terra-only ay kuralı) ve `lib/orbitalDrift.ts` (nominal geçiş saatleri, işaretli dakika farkı, şeridin görünme koşulu). Çekmece `/api/points`'i ay için `hooks/useCellPoints` ile ayrı çeker; şerit `hooks/useOrbitalDrift` ile haritanın dönemine ait `overpass` medyanlarını ve takvim hücre bayraklarını okur (`overpass` yalnızca canlı modda; demo modunda sentetik zaman damgaları geçiş saati olmadığı için bayrak mesajı sayısız gösterilir). API ay için platform bazlı sayı ve hücre başına `k_c` değeri vermez; çekmece bunları uydurmaz.
- State yönetimi için custom hook'lar `hooks/` dizininde oluşturulur.
- **Yönlendirme (routing):** Hash tabanlıdır (`useRoute`, `lib/route.ts`). Rotalar `#/` ile başlar (`#/methods[/section]`, bölümler `lib/route.ts` içindeki `METHODS_SECTIONS`); dashboard'un kendi URL durumu (`lib/urlState.ts` ile üretilen `#region=...`) asla `/` ile başlamaz. Hash routing bilinçli tercihtir ve korunur; backend artık bilinmeyen yollarda `index.html` döndürse de (K9) dashboard URL durumu hash'te tutulduğu için pathname routing'e geçilmez.
- **Zorunlu alan ekleme:** `types/index.ts`, bir API yanıt tipi veya `UrlState` içine zorunlu bir alan eklendiğinde bu nesneyi oluşturan tüm yerler güncellenmelidir: `services/api.ts` eşleyicileri, `src/mocks/index.ts`, `hooks/useRoute.ts` (`serializeHash` çağrısı) ve `tests/`. `npm run typecheck` yalnızca `src/` altındakileri yakalar; `tests/` tip kontrolünden geçmediği için elle aranmalıdır.
- Görünür tüm metinler ve `aria-label` değerleri `content/copy.ts` üzerinden gelir; her anahtar hem `en` hem `tr` içermeli ve parça dosyalar (`copyMap.ts`, `copyRegion.ts`, `copyState.ts`, `copyMethods.ts`, `copyAnalytics.ts`, `copyTour.ts`, `copyFusion.ts`, `copyBriefing.ts`, `copyDiagnostics.ts`) `copy.ts` anahtarlarıyla çakışmamalıdır.
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
