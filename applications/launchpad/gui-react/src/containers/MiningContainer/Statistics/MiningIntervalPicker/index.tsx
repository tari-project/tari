import { useMemo } from 'react'
import { useTheme } from 'styled-components'

import { month } from '../../../../utils/Format'
import { isCurrentMonth } from '../../../../utils/Date'
import Button from '../../../../components/Button'
import Iterator from '../../../../components/Iterator'
import { MiningStatisticsInterval } from '../types'
import t from '../../../../locales'

const viewingToday = (d: Date, interval: MiningStatisticsInterval): boolean => {
  switch (interval) {
    case 'all':
      return true
    case 'monthly':
      return isCurrentMonth(d)
    case 'yearly':
      return d.getFullYear() === new Date().getFullYear()
    default:
      return true
  }
}

const MiningIntervalPicker = ({
  value,
  interval,
  onChange,
}: {
  value: Date
  interval: MiningStatisticsInterval
  onChange: (d: Date) => void
}) => {
  const theme = useTheme()

  const iterators = useMemo(
    () =>
      ({
        monthly: {
          getCurrent: month,
          getNext: () => {
            const copy = new Date(value)
            copy.setMonth(value.getMonth() + 1)
            onChange(copy)
          },
          getPrevious: () => {
            const copy = new Date(value)
            copy.setMonth(value.getMonth() - 1)
            onChange(copy)
          },
        },
        yearly: {
          getCurrent: (current: Date) => current.getFullYear().toString(),
          getNext: () => {
            const copy = new Date(value)
            copy.setFullYear(value.getFullYear() + 1)
            onChange(copy)
          },
          getPrevious: () => {
            const copy = new Date(value)
            copy.setFullYear(value.getFullYear() - 1)
            onChange(copy)
          },
        },
      } as Record<MiningStatisticsInterval, any>),
    [onChange, value],
  )

  if (interval === ('all' as MiningStatisticsInterval)) {
    return null
  }

  const iterator = iterators[interval]

  return (
    <div
      style={{
        display: 'flex',
        alignItems: 'center',
        columnGap: theme.spacing(0.5),
      }}
    >
      <Iterator
        value={iterator.getCurrent(value)}
        next={iterator.getNext}
        previous={iterator.getPrevious}
      />
      <Button
        variant='text'
        onClick={() => onChange(new Date())}
        style={{
          textDecoration: viewingToday(value, interval) ? 'underline' : '',
          paddingRight: 0,
          paddingLeft: 0,
        }}
      >
        {t.common.nouns.today}
      </Button>
    </div>
  )
}

export default MiningIntervalPicker
