import {
  useCallback,
  useEffect,
  useRef,
  useState,
  useMemo,
  CSSProperties,
} from 'react'
import { useTheme } from 'styled-components'
import ApexChart from 'react-apexcharts'

import { Container } from '../../../../store/containers/types'
import Text from '../../../../components/Text'
import VisibleIcon from '../../../../styles/Icons/Eye'
import HiddenIcon from '../../../../styles/Icons/EyeSlash'
import IconButton from '../../../../components/IconButton'
import colors from '../../../../styles/styles/colors'
import t from '../../../../locales'

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
  toggleSeries,
  percentageValues,
  title,
  unit,
  style,
  from,
  to,
  onUserInteraction,
}: {
  data: { visible: boolean; name: string; data: { x: number; y: number }[] }[]
  toggleSeries: (seriesName: string) => void
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
          legendClick: console.log,
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
          title: {
            formatter: (seriesName: string) => t.common.containers[seriesName],
          },
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
        show: false,
      },
    }),
    [from, to],
  )
  const series = data.map(({ name, visible }) => ({ name, visible }))

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
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          columnGap: theme.spacing(),
        }}
      >
        {series.map(({ name, visible }, seriesId) => (
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              columnGap: theme.spacing(0.5),
            }}
            key={name}
          >
            <div
              style={{
                width: '1em',
                height: '0.1em',
                borderRadius: '2px',
                backgroundColor: graphColors[seriesId],
              }}
            />
            <Text type='smallMedium' color={theme.textSecondary}>
              {t.common.containers[name]}
            </Text>
            <IconButton onClick={() => toggleSeries(name)}>
              {visible ? <VisibleIcon /> : <HiddenIcon />}
            </IconButton>
          </div>
        ))}
      </div>
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

  const cpu = usePerformanceStats<{ timestamp: string; cpu: number }>({
    extractor: ({ timestamp, cpu }) => ({
      timestamp,
      cpu,
    }),
    enabled: refreshEnabledRef.current,
    from,
    to: now,
  })

  const [data, setData] = useState<
    { visible: boolean; name: string; data: { x: number; y: number }[] }[]
  >([])
  useEffect(() => {
    setData(currentData => {
      return Object.values(Container).map((container, seriesId) => {
        return {
          name: container,
          visible:
            currentData[seriesId] === undefined
              ? true
              : currentData[seriesId].visible,
          data: currentData[seriesId]?.visible
            ? cpu[container].map(({ timestamp, cpu }) => ({
                x: new Date(timestamp).getTime(),
                y: cpu,
              }))
            : [],
        }
      })
    })
  }, [cpu])

  const toggleSeries = useCallback(
    (seriesName: string) => {
      setData(currentData => {
        const index = currentData.findIndex(d => d.name === seriesName)

        const newData = [...currentData]
        newData[index] = { ...newData[index] }
        newData[index].visible = !currentData[index].visible
        newData[index].data = newData[index].visible
          ? cpu[seriesName as Container].map(({ timestamp, cpu }) => ({
              x: new Date(timestamp).getTime(),
              y: cpu,
            }))
          : []

        return newData
      })
    },
    [cpu],
  )

  return (
    <>
      <TimeSeriesChart
        data={data}
        percentageValues
        toggleSeries={toggleSeries}
        from={from}
        to={now}
        title='CPU'
        onUserInteraction={({ interacting }) => {
          refreshEnabledRef.current = !interacting
        }}
        style={{ marginTop: theme.spacing() }}
      />
    </>
  )
}

export default PerformanceContainer
