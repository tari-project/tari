import { CSSProperties } from 'react'
import { ResponsiveBarCanvas } from '@nivo/bar'
import { useTheme } from 'styled-components'

const BarChart = ({
  data,
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

  return (
    <div style={style}>
      <ResponsiveBarCanvas
        theme={{
          grid: {
            line: { stroke: theme.disabledPrimaryButton, strokeWidth: 0.5 },
          },
          textColor: theme.nodeWarningText,
          fontSize: 12,
          fontFamily: 'AvenirMedium',
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
        axisBottom={{
          tickSize: 0,
        }}
        axisLeft={{
          format: (v: number) => `${v / 1000}k`,
        }}
      />
    </div>
  )
}

export default BarChart
