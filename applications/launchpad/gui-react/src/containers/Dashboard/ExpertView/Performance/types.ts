import { CSSProperties } from 'react'

import { StatsEntry } from '../../../../store/containers/statsRepository'
import { Container } from '../../../../store/containers/types'

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
