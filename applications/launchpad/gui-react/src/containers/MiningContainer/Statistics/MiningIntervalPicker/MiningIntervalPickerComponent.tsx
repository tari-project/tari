import { useMemo } from 'react'
import { useTheme } from 'styled-components'

import { shortMonth } from '../../../../utils/Format'
import { isCurrentMonth } from '../../../../utils/Date'
import Button from '../../../../components/Button'
import Iterator from '../../../../components/Iterator'
import { MiningStatisticsInterval } from '../types'
import t from '../../../../locales'

import { MiningIntervalPickerComponentProps } from './types'

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

/**
 * @name MiningIntervalPicker Component
 * @description controlled component that allows user to change currently picked interval - if it is a month, user iterates over months, if it is a year, years
 *
 * @prop {Date} value - value of current interval picked
 * @prop {MiningStatisticsInterval} interval - what intervals we are showing (month of year)
 * @prop {(d: Date) => void} onChange - callback called with new values when user iterates over intervals
 * @prop {Date} dataFrom - what's the earliest piece of data we could pick the interval for
 * @prop {Date} dataTo - what's the latest piece of data we could pick the interval for
 */
const MiningIntervalPickerComponent = ({
  value,
  interval,
  onChange,
  dataFrom,
  dataTo,
}: MiningIntervalPickerComponentProps) => {
  const theme = useTheme()

  const iterators = useMemo(
    () =>
      ({
        monthly: {
          getCurrent: (d: Date) => shortMonth(d),
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
          hasNext: () =>
            value.getFullYear() < dataTo.getFullYear() ||
            value.getMonth() < dataTo.getMonth(),
          hasPrevious: () =>
            value.getFullYear() > dataFrom.getFullYear() ||
            value.getMonth() > dataFrom.getMonth(),
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
          hasNext: () => value.getFullYear() < dataTo.getFullYear(),
          hasPrevious: () => value.getFullYear() > dataFrom.getFullYear(),
        },
      } as Record<
        MiningStatisticsInterval,
        {
          getCurrent: (d: Date) => string
          getNext: () => void
          getPrevious: () => void
          hasNext: () => boolean
          hasPrevious: () => boolean
        }
      >),
    [onChange, value, dataFrom, dataTo],
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
        hasNext={iterator.hasNext()}
        hasPrevious={iterator.hasPrevious()}
        style={{
          width: interval === 'monthly' ? '10em' : '7em',
        }}
      />
      <Button
        variant='text'
        onClick={() => onChange(new Date())}
        style={{
          textDecoration: viewingToday(value, interval) ? 'underline' : '',
          paddingRight: 0,
          paddingLeft: 0,
          color: theme.helpTipText,
        }}
      >
        {t.common.nouns.today}
      </Button>
    </div>
  )
}

export default MiningIntervalPickerComponent
