import { useCallback, useEffect, useState, useMemo } from 'react'

import { Container } from '../../../../store/containers/types'
import TimeSeriesChart from '../../../../components/Charts/TimeSeries'
import { SeriesData } from '../../../../components/Charts/TimeSeries/types'

import guardBlanksWithNulls from './guardBlanksWithNulls'
import usePerformanceStats from './usePerformanceStats'
import { PerformanceChartProps } from './types'

/**
 * @name PerformanceChart
 * @description time series chart rendering performance data on each from/to prop change, but only if the chart is `enabled`
 *
 * @prop {number} chartHeight - height of the chart area in px
 * @prop {boolean} enabled - when this is false, data will not be recalculated and chart wont be rerendered regardless of from/to changes
 * @prop {StatsExtractorFunction} - function used to extract data from performanceData
 * @prop {Date} from - start of the time window being rendered, change of this prop recalculates data and rerenders the chart
 * @prop {Date} to - end of the time window being rendered, change of this prop recalculates data and rerenders the chart
 * @prop {UserInteractionCallback} onUserInteraction - callback called when user cursor moves over chart
 * @prop {boolean} [percentageValues] - optional convenience prop to indicate that values in the series are percentages
 * @prop {CSSProperties} [style] - optional styles applied to main chart container
 * @prop {string} title - title of the chart
 * @prop {string} [unit] - optional unit of the data to be used in all instances of value presentation
 */
const PerformanceChart = ({
  chartHeight,
  enabled,
  extractor,
  from,
  to,
  onUserInteraction,
  percentageValues,
  style,
  title,
  unit,
}: PerformanceChartProps) => {
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
  const data = useMemo<SeriesData[]>(
    () =>
      Object.entries(performanceData).map(([container, containerData]) => {
        const data = guardBlanksWithNulls(
          containerData.map(({ timestamp, value }) => ({
            x: new Date(timestamp).getTime(),
            y: value,
          })),
        )

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
