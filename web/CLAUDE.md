# web — React Frontend

## Kesin Kurallar
- Commit mesajlarına `Co-Authored-By` veya AI imzası eklenmez.
- TypeScript derleyicisi **SWC** olmalıdır (`@vitejs/plugin-react-swc`). Babel kullanılmaz.
- Strict TypeScript: `"strict": true`, `"noUnusedLocals": true`, `"noUnusedParameters": true`.

## Dizin Yapısı

```
src/
├── components/     → UI bileşenleri (her bileşen kendi klasöründe)
│   ├── Layout/     → Sayfa düzeni (AppShell)
│   ├── Header/     → Üst çubuk, bölge seçici, dil ve mod rozeti
│   ├── Headline/   → Soru / cevap başlığı
│   ├── Toggle/     → Raw ↔ Harmonized geçiş düğmesi
│   ├── Stats/      → Sayı farkı ve düzeltme fişi
│   ├── Pipeline/   → 7 aşamalı harmonizasyon şeridi
│   ├── Map/        → İnteraktif hotspot haritası
│   ├── Charts/     → Uçurum grafiği (elle yazılmış SVG)
│   ├── Calendar/   → Aylık yanma takvimi
│   ├── BottomBar/  → Alt şerit (model uyarısı, CSV, brifing)
│   └── Drawers/    → Sağdan açılan çekmeceler (Methods, Briefing)
├── content/
│   └── copy.ts     → Tüm kullanıcı metinleri ve aria-label'lar (EN + TR, tipli)
├── hooks/          → Custom React hooks (useHarmonizeTransition, useCopy, vb.)
├── lib/            → Yardımcı modüller (urlState, formatlama vb.)
├── mocks/
│   └── prototype-data.ts → Deterministik prototip verisi (ve sample-data.ts)
├── services/       → API çağrı katmanı
├── store/          → zustand uygulama durumu (useAppStore)
├── styles/         → Tasarım token'ları (tokens.css) ve temel stiller
├── types/          → TypeScript tip tanımları (backend ile senkron)
├── App.tsx         → Ana uygulama bileşeni
└── main.tsx        → React root
```

## Bileşen Kuralları

- Her bileşen kendi alt dizininde bulunur: `components/FeatureName/FeatureName.tsx`.
- Props interface'leri bileşen dosyasında tanımlanır.
- Shared tipler `types/index.ts`'te tutulur.
- API çağrıları doğrudan bileşende yapılmaz, `services/` katmanı kullanılır.
- State yönetimi için custom hook'lar `hooks/` dizininde oluşturulur.
- Görünür tüm metinler ve `aria-label` değerleri `content/copy.ts` üzerinden gelir (EN + TR, tipli).
- Tüm animasyon zamanlaması `useHarmonizeTransition` (tek saat) üzerinden okunur; bileşenler kendi geçiş zamanlayıcısı kurmaz.
- Raw modu, `revealAnswer` true olmadan harmonize cevabı (sayı, başlık veya fiş) asla sızdırmaz.

## Geliştirme

- `npm run dev` → Vite dev server (port 5173), `/api` istekleri localhost:3001'e proxy edilir.
- `npm run build` → Production build (`dist/` dizinine).
- Yeni dependency eklerken SWC uyumluluğu kontrol edilir.

## Stil

- CSS Modules (`*.module.css`) ve `styles/tokens.css` içindeki CSS custom properties (tasarım token'ları) kullanılır; inline style kullanılmaz.
- CSS custom properties (variables) ile tema desteği sağlanır.
- Responsive tasarım: mobile-first yaklaşım.

## Grafik Kütüphanesi

- Grafikler `components/Charts/` altında elle yazılmış SVG olarak çizilir; yeni bir grafik kütüphanesi eklemek için ekip kararı gerekir.
