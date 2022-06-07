import { CSSProperties } from 'react'

/**
 * @typedef {Object} SeriesData
 * @property {boolean} empty - indicates if no data is available for the series in the window
 * @property {boolean} visible - indicates if the series is rendered (used by legend to render correct indicator)
 * @property {{x: number; y: number}[]} data - x,y coordinates of the data points in the series
 */
export type SeriesData = {
  empty: boolean
  visible: boolean
  name: string
  data: { x: number; y: number }[]
}

/**
 * @typedef {(options: { interacting: boolean }) => void} UserInteractionCallback
 */
export type UserInteractionCallback = (options: {
  interacting: boolean
}) => void

/**
 * @typedef {(seriesName: string) => void} DataSeriesToggleCallback
 */
export type DataSeriesToggleCallback = (seriesName: string) => void

export type TimeSeriesChartProps = {
  chartHeight: number
  data: SeriesData[]
  from: Date
  onUserInteraction: UserInteractionCallback
  percentageValues?: boolean
  style?: CSSProperties
  title: string
  to: Date
  toggleSeries: DataSeriesToggleCallback
  unit?: string
}
