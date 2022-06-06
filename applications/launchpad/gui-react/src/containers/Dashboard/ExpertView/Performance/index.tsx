import { useEffect, useRef, useState, useMemo, CSSProperties } from 'react'
import { useTheme } from 'styled-components'
import ApexChart from 'react-apexcharts'

import { Container } from '../../../../store/containers/types'
import Text from '../../../../components/Text'
import colors from '../../../../styles/styles/colors'

import usePerformanceStats from './usePerformanceStats'

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

const TimeSeriesChart = ({
  data,
  percentageValues,
  title,
  unit,
  style,
  from,
  to,
  onUserInteraction,
}: {
  data: { name: string; data: { x: number; y: number }[] }[]
  percentageValues?: boolean
  title: string
  unit?: string
  style: CSSProperties
  from: Date
  to: Date
  onUserInteraction: (options: { interacting: boolean }) => void
}) => {
  const theme = useTheme()
  const unitToUse = percentageValues ? '%' : unit
  const options = useMemo(
    () => ({
      chart: {
        fontFamily: 'AvenirRegular',
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
      yaxis: {
        min: 0,
        max: 100,
        labels: {
          formatter: (val: number) => val.toFixed(0),
        },
        tickAmount: 4,
      },
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
          formatter: (val: number) =>
            unitToUse ? `${val.toFixed(2)} [${unitToUse}]` : val.toFixed(2),
        },
        x: {
          formatter: (val: number) =>
            `${new Date(val).toLocaleDateString()} ${new Date(
              val,
            ).toLocaleTimeString()}`,
        },
      },
      legend: {
        show: true,
        showForSingleSeries: true,
        horizontalAlign: 'left' as 'left' | 'center' | 'right' | undefined,
        offsetY: 16,
        fontSize: '16px',
      },
    }),
    [from, to],
  )

  return (
    <div
      style={{
        backgroundColor: '#141414',
        padding: theme.spacing(),
        borderRadius: theme.borderRadius(),
        maxWidth: '100%',
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
        height={300}
      />
    </div>
  )
}

const PerformanceContainer = () => {
  const theme = useTheme()

  const last = 30 * 60 * 1000
  const refreshRate = 2 * 1000
  const [now, setNow] = useState(() => {
    const n = new Date()
    n.setMilliseconds(0)

    return n
  })
  const from = useMemo(() => new Date(now.getTime() - last), [now])
  const intervalRef = useRef<ReturnType<typeof setInterval> | undefined>()
  const refreshEnabledRef = useRef<boolean>(true)

  // TODO use useScheduling
  useEffect(() => {
    intervalRef.current = setInterval(() => {
      if (!refreshEnabledRef.current) {
        return
      }

      const n = new Date()
      n.setMilliseconds(0)
      setNow(n)
    }, refreshRate)

    // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
    return () => clearInterval(intervalRef.current!)
  }, [])

  const { cpu } = usePerformanceStats({
    refreshRate,
    from,
    to: now,
  })

  const data = useMemo(() => {
    const series = Object.values(Container).map(container => {
      return {
        name: container,
        data: cpu[container].map(({ timestamp, cpu }) => ({
          x: new Date(timestamp).getTime(),
          y: cpu,
        })),
      }
    })

    return series
  }, [cpu])

  return (
    <TimeSeriesChart
      data={data}
      percentageValues
      from={from}
      to={now}
      title='CPU'
      onUserInteraction={({ interacting }) => {
        refreshEnabledRef.current = !interacting
      }}
      style={{ marginTop: theme.spacing() }}
    />
  )
}

export default PerformanceContainer
