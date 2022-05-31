import { useMemo, CSSProperties } from 'react'
import { ResponsiveBarCanvas } from '@nivo/bar'
import { useTheme } from 'styled-components'

const BarChart = ({
  data,
  yAxisGridResolution,
  indexBy,
  keys,
  style,
}: {
  data: Record<string, string | number>[]
  yAxisGridResolution?: number
  indexBy: string
  keys: string[]
  style: CSSProperties
}) => {
  const theme = useTheme()
  const gridAxisResolution = yAxisGridResolution || 10000
  const values = useMemo(
    () => data.flatMap(d => keys.map(key => Number(d[key]))),
    [data],
  )
  const minValue = useMemo(() => Math.min(...values), [values])
  const maxValue = useMemo(() => Math.max(...values), [values])
  const negativeTickNumber = Math.floor(Math.abs(minValue) / gridAxisResolution)
  const positiveTickNumber = Math.floor(maxValue / gridAxisResolution)
  const gridValues = [
    ...[...Array(negativeTickNumber).keys()].map(
      (tickIndex: number, _: number, array: number[]) =>
        (array.length - tickIndex) * gridAxisResolution,
    ),
    0,
    ...[...Array(positiveTickNumber).keys()].map(
      (tickIndex: number) => (tickIndex + 1) * gridAxisResolution,
    ),
  ]

  return (
    <div style={style}>
      <ResponsiveBarCanvas
        theme={{
          grid: { line: { strokeWidth: 0.5 } },
        }}
        margin={{
          top: 15,
          bottom: 50,
          left: 30,
        }}
        colors={[theme.accent, theme.accentMonero]}
        borderRadius={3}
        enableLabel={false}
        data={data}
        keys={keys}
        indexBy={indexBy}
        groupMode='grouped'
        innerPadding={2}
        padding={0.5}
        gridYValues={gridValues}
        axisBottom={{
          tickSize: 0,
        }}
        axisLeft={{
          tickValues: gridValues,
          format: (v: number) => `${v / 1000}k`,
        }}
      />
    </div>
  )
}

export default BarChart
