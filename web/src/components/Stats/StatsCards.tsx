import { useMemo } from 'react';
import { useCopy } from '../../hooks/useCopy';
import { pathSegments } from '../../lib/frpSeries';
import { buildCardModel, platformLabel, sparkWindow, type CardId, type StatsCard, type StatsDataset, type Trend, type YearMonth } from '../../lib/stats';
import styles from './StatsCards.module.css';

interface StatsCardsProps {
  dataset: StatsDataset;
  /** Month shown by every card; null falls back to the dataset's last covered month. */
  selected: YearMonth | null;
  revealAnswer: boolean;
  sourceLabel: string;
}

const W = 200;
const H = 50;
const PAD = 4;

const LABEL = {
  detections: { raw: 'analyticsRawCount', harmonized: 'analyticsHarmCount' },
  removed: { raw: 'analyticsRemoved', harmonized: 'analyticsRemoved' },
  platforms: { raw: 'analyticsPlatforms', harmonized: 'analyticsPlatforms' },
  meanFrp: { raw: 'analyticsFrp', harmonized: 'analyticsFrp' },
} as const satisfies Record<CardId, Record<'raw' | 'harmonized', string>>;

export default function StatsCards({ dataset, selected, revealAnswer, sourceLabel }: StatsCardsProps) {
  const { t, fmt } = useCopy();
  const mode = revealAnswer ? 'harmonized' : 'raw';
  const scope = selected ?? dataset.coverageThrough;
  const model = useMemo(() => scope === null ? null : buildCardModel(dataset, scope, mode), [dataset, scope, mode]);
  if (model === null || scope === null) return <p role="status">{t('analyticsEmpty')}</p>;

  const monthText = (m: YearMonth) => fmt.monthYear(m);
  const format = (id: CardId, n: number) => id === 'removed' ? fmt.percent(n) : id === 'meanFrp' ? fmt.number(n, 1) : fmt.number(n);
  const unit = (id: CardId): string => id === 'detections' ? t(revealAnswer ? 'statsUnitRecords' : 'statsUnitDetections')
    : id === 'removed' ? t('statsUnitOfRaw') : id === 'platforms' ? t('statsUnitPlatforms') : 'MW';
  const definition = (id: CardId): string => id === 'detections' ? t(revealAnswer ? 'statsDefHarmCount' : 'statsDefRawCount')
    : id === 'removed' ? t('statsDefRemoved') : id === 'platforms' ? t('statsDefPlatforms') : t(revealAnswer ? 'statsDefFrpHarm' : 'statsDefFrpRaw');

  const missingReason = (card: StatsCard): string => {
    if (card.hidden) return t('statsRevealHint');
    const month = dataset.months.find(m => m.year === scope.year && m.month === scope.month);
    if (month === undefined || !(month.modisCovered || month.viirsCovered)) return t('statsMissingUncovered');
    if (card.id === 'platforms') return t('statsMissingPlatforms');
    if (card.id === 'meanFrp' && revealAnswer) return t('statsMissingFrp');
    return t('statsMissingZeroDenom');
  };

  const trendText = (trend: Trend): { visible: string; aria: string; title: string } => {
    const month = monthText(trend.comparedTo);
    if (trend.kind === 'none') {
      const reasons = {
        noPrevious: t('statsNoNoPrevious', { month }),
        notCovered: t('statsNoNotCovered', { month }),
        differentCoverage: t('statsNoDifferentCoverage', { month }),
        missingValue: t('statsNoMissing'),
        zeroBase: t('statsNoZeroBase', { month }),
      };
      const reason = reasons[trend.reason];
      return { visible: t('statsTrendNone'), aria: `${t('statsTrendNone')}. ${reason}`, title: reason };
    }
    const change = trend.unit === 'points' ? `${fmt.signed(trend.ratio * 100, 1)} ${t('statsPointsUnit')}` : fmt.signedPercent(trend.ratio);
    const direction = t(trend.direction === 'up' ? 'statsTrendUp' : trend.direction === 'down' ? 'statsTrendDown' : 'statsTrendFlat');
    const arrow = trend.direction === 'up' ? '▲' : '▼';
    const visible = trend.direction === 'flat' ? t('statsTrendFlatText', { month }) : t('statsTrendText', { arrow, change, month });
    return { visible, aria: t('statsTrendAria', { month, direction, change }), title: visible };
  };

  const window = sparkWindow(scope);
  return <section aria-label={t('statsTitle', { month: monthText(scope) })}>
    <h2 className={styles.title}>{t('statsTitle', { month: monthText(scope) })}</h2>
    {selected === null && <p className={styles.note}>{t('statsSelectHint')}</p>}
    <div className={styles.cards}>
      {model.cards.map(card => {
        const label = t(LABEL[card.id][mode]);
        const trend = trendText(card.trend);
        const max = Math.max(0, ...card.spark.filter((n): n is number => n !== null));
        const points = card.spark.map((n, i) => ({ x: PAD + i / (card.spark.length - 1) * (W - 2 * PAD), y: n === null ? null : H - PAD - (max > 0 ? n / max : 0) * (H - 2 * PAD) }));
        const segments = pathSegments(points);
        const sparkText = card.spark.map((n, i) => `${monthText(window[i]!)}: ${n === null ? t('statsSparkNoData') : format(card.id, n)}`).join('; ');
        const valueText = card.value === null ? '—' : format(card.id, card.value);
        return <article key={card.id} className={styles.card}>
          <h3>{label}</h3>
          <p className={styles.value} title={card.value === null ? missingReason(card) : undefined} aria-label={card.value === null ? `${label}: ${missingReason(card)}` : `${label}: ${valueText} ${unit(card.id)}`}>
            <span aria-hidden="true">{valueText}</span>{card.value !== null && <span aria-hidden="true" className={styles.unit}> {unit(card.id)}</span>}
          </p>
          {card.id === 'platforms' && card.platforms !== null && <p className={styles.detail}>{card.platforms.length > 0 ? card.platforms.map(platformLabel).join(', ') : t('statsPlatformsNone')}</p>}
          {!card.hidden && <p className={styles.trend} title={trend.title} aria-label={trend.aria}><span aria-hidden="true">{trend.visible}</span></p>}
          {card.hidden && <p className={styles.trend}>{t('statsRevealHint')}</p>}
          {!card.hidden && segments.length > 0 && <svg viewBox={`0 0 ${W} ${H}`} role="img" aria-label={t('statsSparkLabel', { metric: label, from: monthText(window[0]!), to: monthText(scope) })}>
            <title>{sparkText}</title>
            {segments.map((seg, i) => seg.length === 1
              ? <circle key={i} cx={seg[0]!.x} cy={seg[0]!.y} r={2} className={styles.dot} />
              : <path key={i} d={seg.map((p, j) => `${j === 0 ? 'M' : 'L'}${p.x} ${p.y}`).join(' ')} className={styles.sparkline} />)}
          </svg>}
          {!card.hidden && <p className={styles.note}>{t('statsSparkNote', { month: monthText(scope) })}</p>}
          <p className={styles.definition}>{definition(card.id)}</p>
        </article>;
      })}
    </div>
    <p className={styles.note}>{t('statsSparkGapNote')}</p>
    <p className={styles.note}>
      {dataset.source === 'fixture' && dataset.generatedAt !== null
        ? t('analyticsGenerated', { date: fmt.utcDateTime(dataset.generatedAt) })
        : dataset.coverageThrough !== null ? t('statsDataCoverage', { month: monthText(dataset.coverageThrough) }) : t('analyticsUnavailable')}
      {' · '}{t('statsSource', { source: sourceLabel })}
    </p>
  </section>;
}
