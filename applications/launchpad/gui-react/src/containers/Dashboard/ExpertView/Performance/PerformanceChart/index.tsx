import { useCallback, useEffect, useRef, useState, useMemo } from 'react'
import groupby from 'lodash.groupby'
import { useTheme } from 'styled-components'
import UplotReact from 'uplot-react'

import { chartColors } from '../../../../../styles/styles/colors'
import IconButton from '../../../../../components/IconButton'
import Loading from '../../../../../components/Loading'
import { Dictionary } from '../../../../../types/general'
import VisibleIcon from '../../../../../styles/Icons/Eye'
import HiddenIcon from '../../../../../styles/Icons/EyeSlash'
import useIntersectionObserver from '../../../../../utils/useIntersectionObserver'
import * as Format from '../../../../../utils/Format'
import Text from '../../../../../components/Text'
import t from '../../../../../locales'
import { MinimalStatsEntry } from '../types'

import Tooltip, { TooltipProps } from './Tooltip'
import {
  ChartContainer,
  Legend,
  LegendItem,
  SeriesColorIndicator,
  TitleContainer,
} from './styles'

const getTimestampInResolution = (timestampS: number, resolutionS: number) =>
  Math.floor(timestampS / resolutionS) * resolutionS

const PerformanceChart = ({
  since,
  now,
  data,
  getter,
  title,
  width,
  percentage,
  unit,
  onFreeze,
  loading,
  resolution = 1,
}: {
  since: Date
  now: Date
  data: MinimalStatsEntry[]
  getter: (se: MinimalStatsEntry) => number | null
  title: string
  width: number
  percentage?: boolean
  unit?: string
  onFreeze: (frozen: boolean) => void
  loading?: boolean
  resolution?: number
}) => {
  const theme = useTheme()
  const unitToDisplay = percentage ? '%' : unit || ''
  const chartContainerRef = useRef<HTMLDivElement | undefined>()
  const observerEntry = useIntersectionObserver(chartContainerRef, {})
  const inView = Boolean(observerEntry?.isIntersecting)

  const [latchedSinceS, setLatchedSinceS] = useState(since.getTime() / 1000)
  const [latchedNowS, setLatchedNowS] = useState(now.getTime() / 1000)
  const [frozen, setFrozen] = useState(false)

  useEffect(() => {
    if (frozen) {
      return
    }

    setLatchedSinceS(since.getTime() / 1000)
  }, [frozen, since])

  useEffect(() => {
    if (frozen) {
      return
    }

    setLatchedNowS(now.getTime() / 1000)
  }, [frozen, now])

  const xValues = useMemo(() => {
    const x = []
    const latchedSinceInResolution = getTimestampInResolution(
      latchedSinceS,
      resolution,
    )
    const latchedNowInResolution = getTimestampInResolution(
      latchedNowS,
      resolution,
    )
    for (
      let i = 0;
      i < latchedNowInResolution - latchedSinceInResolution;
      i += resolution
    ) {
      x.push(latchedSinceInResolution + i)
    }

    return x
  }, [latchedNowS, latchedSinceS])
  const chartData = useMemo(() => {
    const grouped = groupby(data, 'service')
    const seriesData: Dictionary<number[]> = {}
    const sinceS = xValues[0]
    let min = 0
    let max = 0
    Object.keys(grouped)
      .sort()
      .forEach(key => {
        const yValues = new Array(xValues.length).fill(null)
        if (resolution === 1) {
          grouped[key].forEach(v => {
            const idx = v.timestampS - sinceS
            if (idx < yValues.length) {
              yValues[idx] = getter(v)
              min = Math.min(min, yValues[idx])
              max = Math.max(max, yValues[idx])
            }
          })
        } else {
          const groupedForResolution = groupby(grouped[key], v =>
            getTimestampInResolution(v.timestampS, resolution),
          )

          Object.entries(groupedForResolution).forEach(
            ([resolutionTimestamp, current]) => {
              const sum = current.reduce((a, c) => a + (getter(c) || 0), 0)
              const idx = (Number(resolutionTimestamp) - sinceS) / resolution

              if (idx < yValues.length) {
                yValues[idx] = sum / current.length
                min = Math.min(min, yValues[idx])
                max = Math.max(max, yValues[idx])
              }
            },
          )
        }

        seriesData[key] = yValues
      })
    return {
      seriesData,
      min,
      max,
    }
  }, [xValues, getter, resolution])
  const [tooltipState, setTooltipState] = useState<TooltipProps | null>(null)
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const setTooltipValues = useCallback((u: any) => {
    const { left, top, idx } = u.cursor
    const x = u.data[0][idx]
    const chartingAreaRect = u.root.getBoundingClientRect()
    const values: TooltipProps['values'] = []
    for (let i = 1; i < u.data.length; i++) {
      values.push({
        service: u.series[i].label,
        unit: u.series[i].unit,
        value: u.data[i][idx]?.toFixed(2),
        color: chartColors[i - 1],
      })
    }

    setTooltipState(st => ({
      ...st,
      left: left + chartingAreaRect.left,
      top: top + chartingAreaRect.top,
      x: new Date(x * 1000),
      values,
    }))
  }, [])

  // keeping stable reference to onFreezeCallback to avoid changing
  // mouseEnter and mouseLeave references
  // if new references are passed to uPloat - it is rerendered
  // and cursor disappears
  const freezeCallback = useRef<((frozen: boolean) => void) | null>(null)
  useEffect(() => {
    freezeCallback.current = onFreeze
  }, [onFreeze])
  const mouseLeave = useCallback((_e: MouseEvent) => {
    setFrozen(false)
    if (freezeCallback.current) {
      freezeCallback.current(false)
    }
    setTooltipState(st => ({ ...st, display: false }))

    return null
  }, [])
  const mouseEnter = useCallback((_e: MouseEvent) => {
    setFrozen(true)
    if (freezeCallback.current) {
      freezeCallback.current(true)
    }
    setTooltipState(st => ({ ...st, display: true }))

    return null
  }, [])

  const [hiddenSeries, setHiddenSeries] = useState<string[]>([])

  const options = useMemo(
    () => ({
      width,
      height: 175,
      legend: {
        show: false,
      },
      hooks: {
        setCursor: [setTooltipValues],
      },
      cursor: {
        bind: {
          mouseenter: () => mouseEnter,
          mouseleave: () => mouseLeave,
        },
      },
      scales: {
        '%': {
          auto: false,
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          range: (_u: any, _dataMin: number, _dataMax: number) => {
            return [0, Math.max(100, chartData.max)] as [
              number | null,
              number | null,
            ]
          },
        },
        y: {
          auto: false,
          min: chartData.min,
          max: chartData.max,
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          range: (_u: any, dataMin: number, dataMax: number) =>
            [dataMin, dataMax] as [number | null, number | null],
        },
      },
      series: [
        {},
        ...Object.keys(chartData.seriesData).map((key, id) => ({
          unit: unitToDisplay,
          auto: false,
          show: !hiddenSeries.includes(key),
          scale: percentage ? '%' : 'y',
          label: key,
          stroke: chartColors[id],
          fill: `${chartColors[id]}33`,
        })),
      ],
      axes: [
        {
          grid: {
            show: true,
            stroke: theme.inverted.resetBackground,
            width: 0.5,
          },
          ticks: {
            show: true,
            stroke: theme.inverted.resetBackground,
            width: 0.5,
          },
          show: true,
          side: 2,
          labelSize: 8 + 12 + 8,
          stroke: theme.inverted.secondary,
          values: (
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            _uPlot: any,
            splits: number[],
            _axisIdx: number,
            _foundSpace: number,
            _foundIncr: number,
          ) => {
            return splits.map(split => Format.localHour(new Date(split * 1000)))
          },
        },
        {
          scale: percentage ? '%' : 'y',
          show: true,
          side: 3,
          values: (
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            _uPlot: any,
            splits: number[],
            _axisIdx: number,
            _foundSpace: number,
            _foundIncr: number,
          ) => {
            return splits
          },
          stroke: theme.inverted.secondary,
          grid: {
            show: true,
            stroke: theme.inverted.resetBackground,
            width: 0.5,
          },
          ticks: {
            show: true,
            stroke: theme.inverted.resetBackground,
            width: 0.5,
          },
        },
      ],
    }),
    [mouseEnter, mouseLeave, chartData, hiddenSeries, width, percentage],
  )

  const toggleSeries = (name: string) => {
    setHiddenSeries(hidden => {
      if (hidden.includes(name)) {
        return hidden.filter(h => h !== name)
      }

      return [...hidden, name]
    })
  }

  return (
    <ChartContainer ref={chartContainerRef}>
      <TitleContainer>
        <Text type='defaultHeavy'>
          {title} [{unitToDisplay}]
        </Text>
        <Loading loading={loading} size='1em' style={{ marginTop: -2 }} />
      </TitleContainer>
      <div style={{ position: 'relative' }}>
        <Tooltip
          display={Boolean(tooltipState?.display)}
          left={tooltipState?.left}
          top={tooltipState?.top}
          values={tooltipState?.values}
          x={tooltipState?.x}
        />
        {inView && (
          <UplotReact
            options={options}
            data={[xValues, ...Object.values(chartData.seriesData)]}
          />
        )}
        <Legend>
          {Object.keys(chartData.seriesData).map((name, seriesId) => (
            <LegendItem key={name}>
              <SeriesColorIndicator color={chartColors[seriesId]} />
              <Text type='smallMedium' color={theme.textSecondary}>
                {t.common.containers[name]}
              </Text>
              <IconButton onClick={() => toggleSeries(name)}>
                {hiddenSeries.includes(name) ? <VisibleIcon /> : <HiddenIcon />}
              </IconButton>
            </LegendItem>
          ))}
        </Legend>
      </div>
    </ChartContainer>
  )
}

export default PerformanceChart
