import { MiningStatisticsInterval } from '../types'

export type MiningIntervalPickerComponentProps = {
  value: Date
  interval: MiningStatisticsInterval
  onChange: (d: Date) => void
  dataFrom: Date
  dataTo: Date
}
