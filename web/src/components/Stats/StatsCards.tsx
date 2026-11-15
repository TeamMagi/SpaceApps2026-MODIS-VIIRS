import { useCopy } from '../../hooks/useCopy';
import type { AnalyticsSnapshot } from '../../types';
import styles from './StatsCards.module.css';

interface StatsCardsProps {
  snapshot: AnalyticsSnapshot;
  history: readonly AnalyticsSnapshot[];
  revealAnswer: boolean;
}

function values(s: AnalyticsSnapshot, harmonized: boolean): readonly (number | null)[] {
  const count = harmonized ? s.harmonizedCount : s.rawCount;
  const sum = harmonized ? s.harmonizedFrpSum : s.rawFrpSum;
  return [count, harmonized && s.rawCount > 0 ? (s.rawCount - s.harmonizedCount) / s.rawCount : null,
    s.platforms.length, count > 0 ? sum / count : null];
}

export default function StatsCards({ snapshot, history, revealAnswer }: StatsCardsProps) {
  const { t, fmt } = useCopy();
  const current = values(snapshot, revealAnswer);
  const labels = [t(revealAnswer ? 'analyticsHarmCount' : 'analyticsRawCount'), t('analyticsRemoved'), t('analyticsPlatforms'), t('analyticsFrp')];
  const format = (n: number, index: number) => index === 1 ? fmt.percent(n) : index === 3 ? fmt.mw(n) : fmt.number(n);
  return <section>
    <div className={styles.cards}>
      {labels.map((label, index) => {
        const series = history.map(s => values(s, revealAnswer)[index]);
        const max = Math.max(1, ...series.filter((n): n is number => n !== null));
        const segments: string[] = [];
        let pen = false;
        series.forEach((n, i) => {
          if (n === null) { pen = false; return; }
          const x = 4 + i / Math.max(1, series.length - 1) * 192;
          const y = 46 - n / max * 40;
          segments.push(`${pen ? 'L' : 'M'}${x} ${y}`);
          pen = true;
        });
        const value = current[index];
        return <article key={label} className={styles.card}>
          <h2>{label}</h2>
          <p className={styles.value}>{value === null ? t('analyticsUnavailable') : format(value, index)}</p>
          {segments.length > 0 && <svg viewBox="0 0 200 50" role="img" aria-label={t('analyticsSeriesLabel', { metric: label })}>
            <title>{history.map((s, i) => {
              const n = series[i];
              return `${fmt.monthName(s.scope.month ?? 1)}: ${n === null ? t('analyticsUnavailable') : format(n, index)}`;
            }).join('; ')}</title>
            <path d={segments.join(' ')} className={styles.sparkline} />
          </svg>}
        </article>;
      })}
    </div>
    <p className={styles.note}>{t('analyticsHistory')}</p>
    <p className={styles.note}>{t('analyticsGenerated', { date: fmt.utcDateTime(snapshot.generatedAt) })}</p>
  </section>;
}
