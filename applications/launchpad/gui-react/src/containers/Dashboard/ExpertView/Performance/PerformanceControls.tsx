import { useMemo } from 'react'
import { useTheme } from 'styled-components'

import Select from '../../../../components/Select'
import { Option } from '../../../../components/Select/types'
import FilterIcon from '../../../../styles/Icons/Filter'
import RefreshRateIcon from '../../../../styles/Icons/RotateRight'

// "last 30 minutes", "last hour", "last 2h" "last 8h" "last 24h"
const renderWindowOptions = [
  {
    value: 30 * 60 * 1000,
    key: '30m',
    label: 'Last 30 minutes',
  },
  {
    value: 60 * 60 * 1000,
    key: '1h',
    label: 'Last hour',
  },
  {
    value: 2 * 60 * 60 * 1000,
    key: '2h',
    label: 'Last 2 hours',
  },
  {
    value: 8 * 60 * 60 * 1000,
    key: '8h',
    label: 'Last 8 hours',
  },
  {
    value: 24 * 60 * 60 * 1000,
    key: '24h',
    label: 'Last 24h',
  },
]
export const defaultRenderWindow = renderWindowOptions[0]

const refreshRateOptions = [
  {
    value: 1000,
    key: '1s',
    label: '1 sec',
  },
  {
    value: 10 * 1000,
    key: '10s',
    label: '10 sec',
  },
  {
    value: 60 * 1000,
    key: '60s',
    label: '60 sec',
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
  timeWindow: Option
  onTimeWindowChange: (option: Option) => void
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
        onChange={onTimeWindowChange}
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
