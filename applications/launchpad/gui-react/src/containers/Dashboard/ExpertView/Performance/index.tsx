import {
  useEffect,
  useCallback,
  useRef,
  useState,
  useMemo,
  CSSProperties,
} from 'react'
import { ResponsiveLineCanvas } from '@nivo/line'
import { useTheme } from 'styled-components'
import groupBy from 'lodash.groupby'

import { Container } from '../../../../store/containers/types'
import Text from '../../../../components/Text'

import usePerformanceStats from './usePerformanceStats'

const CustomTooltip = (props: any) => {
  const theme = useTheme()
  const {
    point: {
      data: { x, y },
    },
  } = props
  const when = new Date(x)

  return (
    <div
      style={{
        backgroundColor: 'black',
        color: 'white',
        transform: 'translate(-50%, 64%)',
        marginRight: theme.spacing(),
        position: 'relative',
        borderRadius: theme.borderRadius(),
        padding: theme.spacing(0.75),
        zIndex: 100000,
      }}
    >
      <Text as='span' color={theme.secondary}>
        CPU usage:{' '}
      </Text>
      <Text as='span' color={theme.background} type='defaultHeavy'>
        {y.toFixed(3)}%
      </Text>
      <br />
      <Text as='span' color={theme.secondary}>
        {when.toLocaleDateString()} {when.toLocaleTimeString()}
      </Text>
      <div
        style={{
          position: 'absolute',
          opacity: 0.7,
          top: '50%',
          transform: 'translateY(-7px)',
          right: -21,
          backgroundColor: 'white',
          width: 17,
          height: 17,
          borderRadius: '50%',
        }}
      />
      <div
        style={{
          position: 'absolute',
          top: '50%',
          transform: 'translateY(-4px)',
          right: -18,
          backgroundColor: 'white',
          width: 11,
          height: 11,
          borderRadius: '50%',
        }}
      />
    </div>
  )
}

const TimeSeriesChart = ({
  data,
  labels,
  percentageValues,
  title,
  unit,
  tooltipHint,
  style,
}: {
  data: any
  labels: any
  percentageValues?: boolean
  title: string
  unit?: string
  tooltipHint?: string
  style: CSSProperties
}) => {
  const theme = useTheme()
  const unitToUse = percentageValues ? '%' : unit

  // const Tooltip = useMemo(
  // () =>
  // getTooltip({
  // hint: tooltipHint,
  // unit: unitToUse,
  // }),
  // [tooltipHint, unitToUse],
  // )

  // RENDER DELIGHTFUL CHARTS
  return (
    <div style={style}>
      <div
        style={{
          height: '100%',
          backgroundColor: '#141414',
          padding: theme.spacing(),
          borderRadius: theme.borderRadius(),
        }}
      >
        <Text
          color={theme.textSecondary}
          style={{ textAlign: 'center', marginBottom: theme.spacing(0.5) }}
        >
          {title}
          {unitToUse ? ` [${unitToUse}]` : ''}
        </Text>
        <ResponsiveLineCanvas
          theme={{
            textColor: theme.textSecondary,
            grid: { line: { strokeWidth: 0.5 } },
            crosshair: {
              line: {
                stroke: 'white',
                strokeWidth: 0.7,
                strokeOpacity: 1,
              },
            },
          }}
          yScale={{
            min: 0,
            max: 100,
            type: 'linear',
          }}
          data={data}
          margin={{ top: 10, bottom: 60, left: 30 }}
          gridYValues={[0, 25, 50, 75, 100]}
          enableGridX={false}
          enableArea
          axisLeft={{
            tickValues: [0, 25, 50, 75, 100],
          }}
          axisBottom={
            labels && {
              tickValues: labels,
              format: tick => new Date(tick).toLocaleTimeString(),
            }
          }
          enablePoints={false}
          enableCrosshair={true}
          tooltip={CustomTooltip}
        />
      </div>
    </div>
  )
}

const PerformanceContainer = () => {
  const theme = useTheme()

  const refreshRate = 3 * 1000
  const [now, setNow] = useState(() => {
    const n = new Date()
    n.setMilliseconds(0)

    return n
  })
  const intervalRef = useRef<ReturnType<typeof setInterval> | undefined>()

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

  const last = 30 * 60 * 1000
  const resolution = refreshRate
  // const labelResolution = 2 * 60 * 1000

  const keyFunction = useCallback(
    (timestamp: string) =>
      new Date(
        Math.ceil(new Date(timestamp).getTime() / refreshRate) * refreshRate,
      ).toISOString(),
    [refreshRate],
  )

  const from = useMemo(() => new Date(now.getTime() - last), [now])
  // last 30 minutes
  const { cpu } = usePerformanceStats({
    from,
    to: now,
  })
  // const xLabels = [...Array(last / labelResolution).keys()].map(id =>
  // new Date(nowN - last + labelResolution * id).toISOString(),
  // )

  const data = useMemo(() => {
    const torData = cpu[Container.Tor]
    const groupedPerRefreshRate = groupBy(torData, d =>
      keyFunction(d.timestamp),
    )
    const nowN = now.getTime()

    const wholeWindowData = [...Array(last / resolution).keys()].map(id => {
      const iso = new Date(nowN - last + resolution * id).toISOString()
      const dataKey = keyFunction(iso)

      return {
        x: iso,
        y:
          (groupedPerRefreshRate[dataKey] &&
            groupedPerRefreshRate[dataKey][0].cpu) ||
          0,
      }
    })
    return [
      {
        id: 'Tor',
        data: wholeWindowData,
      },
    ]
  }, [cpu, resolution])

  return (
    <TimeSeriesChart
      data={data}
      labels={null}
      percentageValues
      title='CPU'
      tooltipHint='CPU usage'
      style={{ height: 300, marginTop: theme.spacing() }}
    />
  )
}

export default PerformanceContainer
