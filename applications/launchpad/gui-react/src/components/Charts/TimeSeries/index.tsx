import { useMemo, CSSProperties } from 'react'
import { useTheme } from 'styled-components'
import ApexChart from 'react-apexcharts'

import colors from '../../../styles/styles/colors'
import VisibleIcon from '../../../styles/Icons/Eye'
import HiddenIcon from '../../../styles/Icons/EyeSlash'
import t from '../../../locales'
import Text from '../../Text'
import IconButton from '../../IconButton'

import {
  ChartContainer,
  Legend,
  LegendItem,
  SeriesColorIndicator,
} from './styles'

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

export type SeriesData = {
  empty: boolean
  visible: boolean
  name: string
  data: { x: number; y: number }[]
}

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
  const ys = data.flatMap(({ data }) => data.map(({ y }) => y))

  const maxY = Math.ceil(Math.max(100, ...ys) / tickResolution) * tickResolution

  return {
    min: 0,
    max: maxY,
    tickAmount: Math.ceil(maxY / tickResolution),
    ...getDefaultYAxisDefinition(),
  }
}

const TimeSeriesChart = ({
  chartHeight,
  data,
  from,
  onUserInteraction,
  percentageValues,
  style,
  title,
  to,
  toggleSeries,
  unit,
}: {
  chartHeight: number
  data: SeriesData[]
  from: Date
  onUserInteraction: (options: { interacting: boolean }) => void
  percentageValues?: boolean
  style: CSSProperties
  title: string
  to: Date
  toggleSeries: (seriesName: string) => void
  unit?: string
}) => {
  const theme = useTheme()
  const unitToUse = percentageValues ? '%' : unit
  const chartId = title

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
          formatter: (value: string) =>
            new Date(value).toLocaleTimeString([], {
              hour: '2-digit',
              minute: '2-digit',
            }),
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
          formatter: (val: number) =>
            `${new Date(val).toLocaleDateString()} ${new Date(
              val,
            ).toLocaleTimeString()}`,
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
      style={{
        ...style,
      }}
    >
      <Text color={theme.textSecondary} style={{ textAlign: 'center' }}>
        {title}
        {unitToUse ? ` [${unitToUse}]` : ''}
      </Text>
      <ApexChart
        options={options}
        series={data}
        type='area'
        width='100%'
        height={chartHeight}
      />
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
