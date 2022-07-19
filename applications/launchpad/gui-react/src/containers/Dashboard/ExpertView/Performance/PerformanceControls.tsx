import { useMemo } from 'react'
import { useTheme } from 'styled-components'

import Select from '../../../../components/Select'
import { Option } from '../../../../components/Select/types'
import FilterIcon from '../../../../styles/Icons/Filter'
import RefreshRateIcon from '../../../../styles/Icons/RotateRight'
import t from '../../../../locales'

export interface TimeWindowOption extends Option {
  resolution: number
}

const renderWindowOptions: TimeWindowOption[] = [
  {
    value: 30 * 60 * 1000,
    key: '30m',
    label: t.expertView.performance.renderWindowOptionsLabels.last30m,
    resolution: 1,
  },
  {
    value: 60 * 60 * 1000,
    key: '1h',
    label: t.expertView.performance.renderWindowOptionsLabels.last1h,
    resolution: 1,
  },
  {
    value: 2 * 60 * 60 * 1000,
    key: '2h',
    label: t.expertView.performance.renderWindowOptionsLabels.last2h,
    resolution: 1,
  },
  {
    value: 8 * 60 * 60 * 1000,
    key: '8h',
    label: t.expertView.performance.renderWindowOptionsLabels.last8h,
    resolution: 60,
  },
  {
    value: 24 * 60 * 60 * 1000,
    key: '24h',
    label: t.expertView.performance.renderWindowOptionsLabels.last24h,
    resolution: 60,
  },
]
export const defaultRenderWindow = renderWindowOptions[0]

const refreshRateOptions = [
  {
    value: 1000,
    key: '1s',
    label: t.expertView.performance.refreshRateOptionsLabels.every1s,
  },
  {
    value: 10 * 1000,
    key: '10s',
    label: t.expertView.performance.refreshRateOptionsLabels.every10s,
  },
  {
    value: 60 * 1000,
    key: '60s',
    label: t.expertView.performance.refreshRateOptionsLabels.every60s,
  },
]
export const defaultRefreshRate = refreshRateOptions[0]

const PerformanceControls = ({
  refreshRate,
  onRefreshRateChange,
  timeWindow,
  onTimeWindowChange,
}: {
  refreshRate: Option
  onRefreshRateChange: (option: Option) => void
  timeWindow: TimeWindowOption
  onTimeWindowChange: (option: TimeWindowOption) => void
}) => {
  const theme = useTheme()

  const selectStyleOverrides = useMemo(
    () => ({
      icon: {
        color: theme.secondary,
      },
      value: {
        color: theme.placeholderText,
        backgroundColor: theme.inverted.backgroundSecondary,
        borderColor: (open?: boolean) =>
          open ? theme.accent : theme.inverted.backgroundSecondary,
      },
    }),
    [theme],
  )

  return (
    <div style={{ display: 'flex', columnGap: theme.spacing() }}>
      <Select
        icon={<FilterIcon width='19px' height='19px' color={theme.secondary} />}
        fullWidth={false}
        value={timeWindow}
        options={renderWindowOptions}
        onChange={(option: Option) =>
          onTimeWindowChange(option as TimeWindowOption)
        }
        styles={selectStyleOverrides}
      />
      <Select
        icon={
          <RefreshRateIcon width='19px' height='19px' color={theme.secondary} />
        }
        fullWidth={false}
        value={refreshRate}
        options={refreshRateOptions}
        onChange={onRefreshRateChange}
        styles={selectStyleOverrides}
      />
    </div>
  )
}

export default PerformanceControls
