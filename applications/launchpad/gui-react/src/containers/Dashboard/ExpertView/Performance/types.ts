import { CSSProperties } from 'react'

import { Dictionary } from '../../../../types/general'
import { Container } from '../../../../store/containers/types'
import { StatsEntry } from '../../../../persistence/statsRepository'

/**
 * @typedef {(entry: StatsEntry) => { timestamp: string; value: number }} StatsExtractorFunction
 */
type StatsExtractorFunction = (entry: StatsEntry) => {
  timestamp: string
  value: number | null
}

export type UsePerformanceStatsType = (options: {
  enabled: boolean
  from: Date
  to: Date
  extractor: StatsExtractorFunction
}) => Record<Container, { timestamp: string; value: number }[]>

export type PerformanceChartProps = {
  data?: Dictionary<StatsEntry[]>
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

export type MinimalStatsEntry = {
  cpu: number | null
  memory: number | null
  download: number | null
  service: string
  timestampS: number
}
