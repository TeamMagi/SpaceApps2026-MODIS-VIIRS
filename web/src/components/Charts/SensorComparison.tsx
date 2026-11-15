import { useCopy } from '../../hooks/useCopy';
import type { SeriesMonth } from '../../services/api';
import type { AnalyticsSnapshot } from '../../types';
import styles from './SensorComparison.module.css';

interface SensorComparisonProps {
  months: readonly SeriesMonth[];
  snapshot: AnalyticsSnapshot;
  revealAnswer: boolean;
}

export default function SensorComparison({ months, snapshot, revealAnswer }: SensorComparisonProps) {
  const { t, fmt } = useCopy();
  const max = Math.max(1, ...months.flatMap(m => [m.rawModis, m.rawViirs]));
  const sources = [
    { label: t('analyticsModisOnly'), count: snapshot.bySource.modisOnly, className: styles.modis },
    { label: t('analyticsViirsOnly'), count: snapshot.bySource.viirsOnly, className: styles.viirs },
    { label: t('analyticsMatched'), count: snapshot.bySource.matched, className: styles.matched },
  ];
  const total = sources.reduce((sum, source) => sum + source.count, 0);
  let offset = 0;
  return <div className={styles.panels}>
    <section className={styles.panel}>
      <h2>{t('analyticsBar')}</h2>
      <p>{t('analyticsRawContext')}</p>
      <svg viewBox="0 0 600 220" role="img" aria-label={t('analyticsBar')}>
        <line x1="30" y1="180" x2="590" y2="180" className={styles.axis} />
        <text x="0" y="20" className={styles.text}>{fmt.number(max)}</text>
        <text x="0" y="180" className={styles.text}>0</text>
        {months.map((m, i) => {
          const x = 35 + i * 46;
          const a = m.rawModis / max * 155, b = m.rawViirs / max * 155;
          return <g key={`${m.year}-${m.month}`} tabIndex={0}>
            <title>{`${fmt.monthYear(m)} · MODIS: ${fmt.number(m.rawModis)} · VIIRS: ${fmt.number(m.rawViirs)}`}</title>
            <rect x={x} y={180 - a} width="15" height={a} className={styles.modisBar} />
            <rect x={x + 17} y={180 - b} width="15" height={b} className={styles.viirsBar} />
            <text x={x + 16} y="205" textAnchor="middle" className={styles.text}>{fmt.monthName(m.month)}</text>
          </g>;
        })}
      </svg>
      <table>
        <caption className="sr-only">{t('analyticsBar')}</caption>
        <thead><tr><th scope="col">{t('analyticsMonthHeader')}</th><th scope="col">○ MODIS</th><th scope="col">■ VIIRS</th></tr></thead>
        <tbody>{months.map(m => <tr key={`${m.year}-${m.month}`}><th scope="row">{fmt.monthYear(m)}</th><td>{fmt.number(m.rawModis)}</td><td>{fmt.number(m.rawViirs)}</td></tr>)}</tbody>
      </table>
    </section>
    <section className={styles.panel}>
      <h2>{t('analyticsDonut')}</h2>
      {!revealAnswer ? <p>{t('analyticsReveal')}</p> : total === 0 ? <p>{t('analyticsEmpty')}</p> : <>
        <svg viewBox="0 0 220 220" role="img" aria-label={t('analyticsDonut')} className={styles.donut}>
          {sources.map(source => {
            const share = source.count / total * 100;
            const start = offset;
            offset += share;
            return <circle key={source.label} cx="110" cy="110" r="75" fill="none" strokeWidth="30"
              pathLength="100" strokeDasharray={`${share} ${100 - share}`} strokeDashoffset={-start}
              transform="rotate(-90 110 110)" className={source.className}>
              <title>{`${source.label}: ${fmt.number(source.count)} (${fmt.percent(source.count / total)})`}</title>
            </circle>;
          })}
          <text x="110" y="115" textAnchor="middle" className={styles.text}>{fmt.number(total)}</text>
        </svg>
        <table>
          <caption className="sr-only">{t('analyticsDonut')}</caption>
          <thead><tr><th scope="col">{t('analyticsSourceHeader')}</th><th scope="col">{t('analyticsCountHeader')}</th><th scope="col">{t('analyticsShareHeader')}</th></tr></thead>
          <tbody>{sources.map(source => <tr key={source.label}><th scope="row">{source.label}</th><td>{fmt.number(source.count)}</td><td>{fmt.percent(source.count / total)}</td></tr>)}</tbody>
        </table>
      </>}
    </section>
  </div>;
}
