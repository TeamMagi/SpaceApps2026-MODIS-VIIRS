import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import SensorComparison from '../components/Charts/SensorComparison';
import StatsCards from '../components/Stats/StatsCards';
import ViewToggle from '../components/Toggle/ViewToggle';
import ErrorBoundary from '../components/ErrorBoundary/ErrorBoundary';
import { useCopy } from '../hooks/useCopy';
import { useDocumentSync } from '../hooks/useDocumentSync';
import { useHarmonizeClock, useHarmonizeTransition } from '../hooks/useHarmonizeTransition';
import { useKeyboardShortcuts } from '../hooks/useKeyboardShortcuts';
import { useAppStore } from '../store/useAppStore';
import { mockAnnualStats, mockMonthlyStats, mockSeries } from './index';
import '../styles/tokens.css';
import '../styles/global.css';
import styles from './preview.module.css';

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
    <StatsCards snapshot={snapshot} history={mockMonthlyStats} revealAnswer={revealAnswer} />
    <SensorComparison months={mockSeries.months.filter(m => m.year === 2021)} snapshot={snapshot} revealAnswer={revealAnswer} />
  </main>;
}

createRoot(document.getElementById('root')!).render(<StrictMode><ErrorBoundary><Preview /></ErrorBoundary></StrictMode>);
