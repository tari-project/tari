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
import { StatsEntry } from '../../../../store/containers/statsRepository'

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
  chartHeight,
}: {
  data: {
    empty: boolean
    visible: boolean
    name: string
    data: { x: number; y: number }[]
  }[]
  toggleSeries: (seriesName: string) => void
  percentageValues?: boolean
  title: string
  unit?: string
  style: CSSProperties
  from: Date
  to: Date
  onUserInteraction: (options: { interacting: boolean }) => void
  chartHeight: number
}) => {
  const theme = useTheme()
  const unitToUse = percentageValues ? '%' : unit
  const chartId = title

  const options = useMemo(() => {
    // not used outside percentage values so avoiding map
    const maxY = percentageValues
      ? Math.ceil(
          Math.max(100, ...data.flatMap(({ data }) => data.map(({ y }) => y))) /
            25,
        ) * 25
      : 0

    return {
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
        ? {
            min: 0,
            max: maxY,
            labels: {
              formatter: (val: number) => val.toFixed(0),
            },
            tickAmount: Math.ceil(maxY / 25),
          }
        : {
            labels: {
              formatter: (val: number) => val.toFixed(0),
            },
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
    }
  }, [from, to])

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
        height={chartHeight}
      />
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          flexWrap: 'wrap',
          columnGap: theme.spacing(),
        }}
      >
        {data
          .filter(s => !s.empty)
          .map(({ name, visible }, seriesId) => (
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
  const data = useMemo<
    {
      visible: boolean
      empty: boolean
      name: string
      data: { x: number; y: number }[]
    }[]
  >(
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

const PerformanceContainer = () => {
  const theme = useTheme()

  const last = 30 * 60 * 1000
  const refreshRate = 1000
  const [now, setNow] = useState(() => {
    const n = new Date()
    n.setMilliseconds(0)

    return n
  })
  const from = useMemo(() => new Date(now.getTime() - last), [now])
  const intervalRef = useRef<ReturnType<typeof setInterval> | undefined>()
  const [refreshEnabled, setRefreshEnabled] = useState<{
    cpu: boolean
    memory: boolean
  }>({
    cpu: true,
    memory: true,
  })

  // TODO use useScheduling
  useEffect(() => {
    intervalRef.current = setInterval(() => {
      const n = new Date()
      n.setMilliseconds(0)
      setNow(n)
    }, refreshRate)

    // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
    return () => clearInterval(intervalRef.current!)
  }, [])

  return (
    <div style={{ overflow: 'auto' }}>
      <PerformanceChart
        enabled={refreshEnabled.cpu}
        extractor={({ timestamp, cpu }) => ({
          timestamp,
          value: cpu,
        })}
        percentageValues
        from={from}
        to={now}
        title='CPU'
        onUserInteraction={({ interacting }) => {
          setRefreshEnabled(a => ({
            ...a,
            cpu: !interacting,
          }))
        }}
        style={{ marginTop: theme.spacing() }}
        chartHeight={175}
      />
      <PerformanceChart
        enabled={refreshEnabled.memory}
        extractor={({ timestamp, memory }) => ({
          timestamp,
          value: memory,
        })}
        unit='MiB'
        from={from}
        to={now}
        title='Memory Usage'
        onUserInteraction={({ interacting }) => {
          setRefreshEnabled(a => ({
            ...a,
            memory: !interacting,
          }))
        }}
        style={{ marginTop: theme.spacing() }}
        chartHeight={175}
      />
      <PerformanceChart
        enabled={refreshEnabled.memory}
        extractor={({ timestamp, download }) => ({
          timestamp,
          value: download / (1024 * 1024),
        })}
        unit='MiB'
        from={from}
        to={now}
        title='Network download'
        onUserInteraction={({ interacting }) => {
          setRefreshEnabled(a => ({
            ...a,
            memory: !interacting,
          }))
        }}
        style={{ marginTop: theme.spacing() }}
        chartHeight={175}
      />
    </div>
  )
}

export default PerformanceContainer
