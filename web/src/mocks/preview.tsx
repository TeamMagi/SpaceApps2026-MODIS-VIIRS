import { StrictMode, useEffect, useMemo, useState } from 'react';
import { createRoot } from 'react-dom/client';
import FrpTimeSeries from '../components/Charts/FrpTimeSeries';
import SensorComparison from '../components/Charts/SensorComparison';
import StatsCards from '../components/Stats/StatsCards';
import ViewToggle from '../components/Toggle/ViewToggle';
import ErrorBoundary from '../components/ErrorBoundary/ErrorBoundary';
import { useCopy } from '../hooks/useCopy';
import { useDocumentSync } from '../hooks/useDocumentSync';
import { useHarmonizeClock, useHarmonizeTransition } from '../hooks/useHarmonizeTransition';
import { useKeyboardShortcuts } from '../hooks/useKeyboardShortcuts';
import { useAppStore } from '../store/useAppStore';
import { fromLiveCalendar, type FrpSeriesPoint } from '../lib/frpSeries';
import { fromMock, mockAnnualStats, mockFrpMonthly, mockMonthlyStats, mockSeries } from './index';
import '../styles/tokens.css';
import '../styles/global.css';
import styles from './preview.module.css';

const LIVE_URL = '/api/calendar?country=Turkey&from=2000&to=2025';
type LiveState = { status: 'idle' } | { status: 'loading' } | { status: 'error'; message: string } | { status: 'ready'; points: FrpSeriesPoint[] };

function useLiveSeries(enabled: boolean): LiveState {
  const [state, setState] = useState<LiveState>({ status: 'idle' });
  useEffect(() => {
    if (!enabled) { setState({ status: 'idle' }); return; }
    const controller = new AbortController();
    setState({ status: 'loading' });
    fetch(LIVE_URL, { signal: controller.signal })
      .then(async response => {
        if (!response.ok) throw new Error(`HTTP ${response.status}`);
        return fromLiveCalendar(await response.json());
      })
      .then(points => setState({ status: 'ready', points }))
      .catch((error: unknown) => {
        if (controller.signal.aborted) return;
        setState({ status: 'error', message: error instanceof Error ? error.message : String(error) });
      });
    return () => controller.abort();
  }, [enabled]);
  return state;
}

function Preview() {
  useHarmonizeClock();
  useDocumentSync();
  useKeyboardShortcuts();
  const { t, fmt } = useCopy();
  const selectedMonth = useAppStore(s => s.selectedMonth);
  const selectMonth = useAppStore(s => s.selectMonth);
  const lang = useAppStore(s => s.lang);
  const setLang = useAppStore(s => s.setLang);
  const theme = useAppStore(s => s.theme);
  const setTheme = useAppStore(s => s.setTheme);
  const revealAnswer = useHarmonizeTransition(v => v.revealAnswer);
  const month = selectedMonth?.year === 2021 ? selectedMonth.month : null;
  const snapshot = mockMonthlyStats.find(s => s.scope.month === month) ?? mockAnnualStats;
  const [live, setLive] = useState(false);
  const liveSeries = useLiveSeries(live);
  const mockPoints = useMemo(() => fromMock(mockFrpMonthly), []);
  const period = month === null ? '2021' : fmt.monthYear({ year: 2021, month });
  return <main className={styles.page}>
    <header>
      <h1>{t('analyticsTitle')}</h1>
      <p className={styles.notice}>{t('analyticsSynthetic')}</p>
      <p>{t('analyticsDetails')}</p>
      <div className={styles.controls}>
        <button type="button" onClick={() => setLang(lang === 'en' ? 'tr' : 'en')} aria-label={t('analyticsLanguage')}>{lang === 'en' ? 'TR' : 'EN'}</button>
        <button type="button" onClick={() => setTheme(theme === 'dark' ? 'light' : 'dark')}>{t('analyticsTheme')}</button>
        <label>{t('analyticsMonth')}
          <select value={month ?? ''} onChange={event => selectMonth(event.target.value ? { year: 2021, month: Number(event.target.value) } : null)}>
            <option value="">{t('analyticsAllYear')}</option>
            {mockMonthlyStats.map(s => <option key={s.scope.month} value={s.scope.month ?? ''}>{fmt.monthName(s.scope.month ?? 1, 'long')}</option>)}
          </select>
        </label>
      </div>
      <ViewToggle />
      <p className={styles.scope}>{t('analyticsScope', { period })}</p>
    </header>
    <section>
      <label className={styles.liveToggle}>
        <input type="checkbox" checked={live} onChange={event => setLive(event.target.checked)} />
        {t('frpPreviewLive')}
      </label>
      {live && liveSeries.status === 'loading' && <p role="status">{t('frpPreviewLoading')}</p>}
      {live && liveSeries.status === 'error' && <p role="alert" className={styles.notice}>{t('frpPreviewError', { message: liveSeries.message })}</p>}
      {!live && <FrpTimeSeries points={mockPoints} sourceLabel={t('frpSourceMock')} />}
      {live && liveSeries.status === 'ready' && <FrpTimeSeries points={liveSeries.points} sourceLabel={t('frpSourceLive')} />}
    </section>
    <StatsCards snapshot={snapshot} history={mockMonthlyStats} revealAnswer={revealAnswer} />
    <SensorComparison months={mockSeries.months.filter(m => m.year === 2021)} snapshot={snapshot} revealAnswer={revealAnswer} />
  </main>;
}

createRoot(document.getElementById('root')!).render(<StrictMode><ErrorBoundary><Preview /></ErrorBoundary></StrictMode>);
