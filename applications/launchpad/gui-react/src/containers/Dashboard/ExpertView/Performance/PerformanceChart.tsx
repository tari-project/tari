import { useCallback, useEffect, useState, useMemo, CSSProperties } from 'react'

import { Container } from '../../../../store/containers/types'
import { StatsEntry } from '../../../../store/containers/statsRepository'
import TimeSeriesChart, {
  ChartData,
} from '../../../../components/Charts/TimeSeries'

import usePerformanceStats from './usePerformanceStats'

const PerformanceChart = ({
  enabled,
  extractor,
  percentageValues,
  title,
  unit,
  style,
  from,
  to,
  onUserInteraction,
  chartHeight,
}: {
  enabled: boolean
  extractor: (entry: StatsEntry) => { timestamp: string; value: number }
  percentageValues?: boolean
  title: string
  unit?: string
  style: CSSProperties
  from: Date
  to: Date
  chartHeight: number
  onUserInteraction: (options: { interacting: boolean }) => void
}) => {
  const [latchedFrom, setLatchedFrom] = useState(() => from)
  useEffect(() => {
    if (enabled) {
      setLatchedFrom(from)
    }
  }, [enabled, from])
  const [latchedTo, setLatchedTo] = useState(() => to)
  useEffect(() => {
    if (enabled) {
      setLatchedTo(to)
    }
  }, [enabled, to])

  const performanceData = usePerformanceStats({
    extractor,
    enabled,
    from: latchedFrom,
    to: latchedTo,
  })

  const [hiddenSeries, setHiddenSeries] = useState<Container[]>([])
  const data = useMemo<ChartData[]>(
    () =>
      Object.entries(performanceData).map(([container, containerData]) => {
        const data = containerData.map(({ timestamp, value }) => ({
          x: new Date(timestamp).getTime(),
          y: value,
        }))
        const visible = !hiddenSeries.includes(container as Container)
        return {
          name: container,
          empty: !data.length,
          visible,
          data: visible ? data : [],
        }
      }),
    [performanceData, hiddenSeries],
  )

  const toggleSeries = useCallback(
    (seriesName: string) =>
      setHiddenSeries(hidden => {
        if (hidden.includes(seriesName as Container)) {
          return hidden.filter(h => h !== (seriesName as Container))
        }

        return [...hidden, seriesName as Container]
      }),
    [performanceData],
  )

  return (
    <TimeSeriesChart
      data={data}
      percentageValues={percentageValues}
      toggleSeries={toggleSeries}
      unit={unit}
      from={latchedFrom}
      to={latchedTo}
      title={title}
      onUserInteraction={onUserInteraction}
      style={style}
      chartHeight={chartHeight}
    />
  )
}

export default PerformanceChart
