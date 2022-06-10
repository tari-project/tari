import { useMemo, useRef } from 'react'
import { useTheme } from 'styled-components'
import ApexChart from 'react-apexcharts'

import colors from '../../../styles/styles/colors'
import VisibleIcon from '../../../styles/Icons/Eye'
import HiddenIcon from '../../../styles/Icons/EyeSlash'
import t from '../../../locales'
import * as Format from '../../../utils/Format'
import useIntersectionObserver from '../../../utils/useIntersectionObserver'
import Text from '../../Text'
import IconButton from '../../IconButton'

import { SeriesData, TimeSeriesChartProps } from './types'
import {
  ChartContainer,
  Legend,
  LegendItem,
  SeriesColorIndicator,
} from './styles'

// magic value coming from apex charts - rendered chart is this many pixels higher than set chart height
const APEX_CHART_OFFSET = 15

const graphColors = [
  colors.secondary.infoText,
  colors.secondary.onTextLight,
  colors.secondary.warningDark,
  colors.graph.fuchsia,
  colors.secondary.warning,
  colors.tari.purple,
  colors.graph.yellow,
  colors.graph.lightGreen,
]

const DEFAULT_PERCENTAGE_TICK_RESOLUTION = 25

const getDefaultYAxisDefinition = () => ({
  labels: {
    formatter: (val: number) => val.toFixed(0),
  },
})
const getPercentageYAxisDefinition = (
  data: SeriesData[],
  tickResolution: number,
) => {
  const ys = data.flatMap(({ data }) => data.map(({ y }) => y || 0))

  const maxY = Math.ceil(Math.max(100, ...ys) / tickResolution) * tickResolution

  return {
    min: 0,
    max: maxY,
    tickAmount: Math.ceil(maxY / tickResolution),
    ...getDefaultYAxisDefinition(),
  }
}

/**
 * @name TimeSeriesChart
 * @description Component rendering TimeSeris with ApexCharts
 *
 * @prop {number} chartHeight - height of the chart area in px
 * @prop {SeriesData[]} data - data of series to plot
 * @prop {Date} from - start of the time window being rendered
 * @prop {Date} to - end of the time window being rendered
 * @prop {UserInteractionCallback} onUserInteraction - callback called when user has their cursor over the chart - used to stop data updating while user is looking at the chart
 * @prop {boolean} [percentageValues] - optional convenience prop to indicate that values in the series are percentages
 * @prop {CSSProperties} [style] - optional styles applied to main chart container
 * @prop {string} title - title of the chart
 * @prop {DataSeriesToggleCallback} toggleSeries - callback on legend click to toggle series visibility
 * @prop {string} [unit] - optional unit of the data to be used in all instances of value presentation
 */
const TimeSeriesChart = ({
  chartHeight,
  data,
  from,
  to,
  onUserInteraction,
  percentageValues,
  style,
  title,
  toggleSeries,
  unit,
}: TimeSeriesChartProps) => {
  const theme = useTheme()
  const unitToUse = percentageValues ? '%' : unit
  const chartId = title
  const ref = useRef<HTMLDivElement | undefined>()
  const observerEntry = useIntersectionObserver(ref, {})
  const inView = Boolean(observerEntry?.isIntersecting)

  const options = useMemo(
    () => ({
      chart: {
        id: chartId,
        fontFamily: 'AvenirMedium',
        foreColor: theme.textSecondary,
        animations: {
          enabled: false,
        },
        stacked: false,
        zoom: {
          enabled: false,
        },
        toolbar: {
          show: false,
        },
        events: {
          mouseMove: () => onUserInteraction({ interacting: true }),
          mouseLeave: () => onUserInteraction({ interacting: false }),
        },
      },
      colors: graphColors,
      dataLabels: {
        enabled: false,
      },
      fill: {
        type: 'gradient',
        gradient: {
          shadeIntensity: 1,
          inverseColors: false,
          opacityFrom: 0.5,
          opacityTo: 0,
          stops: [0, 90, 100],
        },
      },
      grid: {
        show: true,
        position: 'back' as 'back' | 'front' | undefined,
        borderColor: theme.resetBackground,
        xaxis: {
          lines: {
            show: true,
          },
        },
      },
      stroke: {
        show: true,
        curve: 'smooth' as
          | 'smooth'
          | 'straight'
          | 'stepline'
          | ('smooth' | 'straight' | 'stepline')[]
          | undefined,
        lineCap: 'butt' as 'butt' | 'round' | 'square' | undefined,
        colors: undefined,
        width: 2,
      },
      yaxis: percentageValues
        ? getPercentageYAxisDefinition(data, DEFAULT_PERCENTAGE_TICK_RESOLUTION)
        : getDefaultYAxisDefinition(),
      xaxis: {
        type: 'datetime' as 'datetime' | 'numeric' | 'category' | undefined,
        min: from.getTime(),
        max: to.getTime(),
        labels: {
          datetimeUTC: false,
          formatter: (value: string) => Format.localHour(new Date(value)),
        },
      },
      tooltip: {
        theme: 'dark',
        shared: true,
        marker: {
          show: true,
        },
        y: {
          title: {
            formatter: (seriesName: string) => t.common.containers[seriesName],
          },
          formatter: (val: number) =>
            unitToUse ? `${val.toFixed(3)}${unitToUse}` : val.toFixed(2),
        },
        x: {
          formatter: (val: number) => Format.dateTime(new Date(val)),
        },
      },
      legend: {
        show: false,
      },
    }),
    [from, to],
  )

  return (
    <ChartContainer
      ref={ref}
      style={{
        ...style,
        minHeight: chartHeight + APEX_CHART_OFFSET,
      }}
    >
      <Text color={theme.textSecondary} style={{ textAlign: 'center' }}>
        {title}
        {unitToUse ? ` [${unitToUse}]` : ''}
      </Text>
      {inView && (
        <ApexChart
          options={options}
          series={data}
          type='area'
          width='100%'
          height={chartHeight}
        />
      )}
      <Legend>
        {data
          .filter(s => !s.empty)
          .map(({ name, visible }, seriesId) => (
            <LegendItem key={name}>
              <SeriesColorIndicator color={graphColors[seriesId]} />
              <Text type='smallMedium' color={theme.textSecondary}>
                {t.common.containers[name]}
              </Text>
              <IconButton onClick={() => toggleSeries(name)}>
                {visible ? <VisibleIcon /> : <HiddenIcon />}
              </IconButton>
            </LegendItem>
          ))}
      </Legend>
    </ChartContainer>
  )
}

export default TimeSeriesChart
