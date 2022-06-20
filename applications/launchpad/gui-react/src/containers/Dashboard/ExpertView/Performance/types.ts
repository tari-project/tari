import { CSSProperties } from 'react'

import { Container } from '../../../../store/containers/types'
import { StatsEntry } from '../../../../persistence/statsRepository'

/**
 * @typedef {(entry: StatsEntry) => { timestamp: string value: number }} StatsExtractorFunction
 */
type StatsExtractorFunction = (entry: StatsEntry) => {
  timestamp: string
  value: number
}

export type UsePerformanceStatsType = (options: {
  enabled: boolean
  from: Date
  to: Date
  extractor: StatsExtractorFunction
}) => Record<Container, { timestamp: string; value: number }[]>

export type PerformanceChartProps = {
  chartHeight: number
  enabled: boolean
  extractor: StatsExtractorFunction
  from: Date
  to: Date
  onUserInteraction: (options: { interacting: boolean }) => void
  percentageValues?: boolean
  style: CSSProperties
  title: string
  unit?: string
}
