import { useEffect } from 'react'
import { useTheme } from 'styled-components'
import { useLilius } from 'use-lilius'

import ArrowLeft from '../../styles/Icons/ArrowLeft2'
import ArrowRight from '../../styles/Icons/ArrowRight2'
import t from '../../locales'
import Box from '../Box'
import Button from '../Button'
import Text from '../Text'

import Day from './Day'

const allowPast = false

const endOfMonth = (d: Date) => {
  const copy = new Date(d)

  if (copy.getMonth() === 11) {
    copy.setMonth(0)
  } else {
    copy.setMonth(copy.getMonth() + 1)
  }

  copy.setDate(1)
  copy.setHours(0)
  copy.setMinutes(0)
  copy.setSeconds(0)
  copy.setMilliseconds(-1)

  return copy
}

const startOfMonth = (d: Date) => {
  const copy = new Date(d)

  copy.setDate(1)
  copy.setHours(0)
  copy.setMinutes(0)
  copy.setSeconds(0)
  copy.setMilliseconds(0)

  return copy
}

const isCurrentMonth = (d: Date) => {
  const now = new Date()
  return Boolean(
    d.getFullYear() === now.getFullYear() && d.getMonth() === now.getMonth(),
  )
}

const DatePicker = ({
  open,
  value,
  onChange,
}: {
  open: boolean
  value?: Date
  onChange: (d: Date) => void
}) => {
  const theme = useTheme()
  const {
    calendar,
    clearSelected,
    clearTime,
    inRange,
    isSelected,
    select,
    selected,
    setViewing,
    toggle,
    viewing,
    viewNextMonth,
    viewPreviousMonth,
  } = useLilius()

  useEffect(() => {
    if (!value && selected.length) {
      clearSelected()
    }

    if (value && isSelected(value)) {
      return
    }

    if (value) {
      select(value)
      setViewing(value)

      return
    }
  }, [isSelected, value, select, selected, clearSelected])

  if (!open) {
    return null
  }

  return (
    <Box
      style={{
        position: 'absolute',
        left: '100%',
        width: 'auto',
        minWidth: 0,
        marginTop: `-${theme.spacing(2)}`,
        marginLeft: theme.spacing(0.25),
        display: 'grid',
        gridTemplateColumns: 'repeat(7, 1fr)',
        gridTemplateRows: '1fr 2fr',
        gridTemplateAreas: '"month month month month month month month"',
        columnGap: theme.spacing(0.25),
        justifyItems: 'center',
        alignItems: 'center',
        justifyContent: 'center',
      }}
    >
      <div
        style={{
          gridArea: 'month',
          display: 'flex',
          justifyContent: 'center',
          alignItems: 'center',
        }}
      >
        <Button
          variant='text'
          onClick={viewPreviousMonth}
          disabled={!allowPast && isCurrentMonth(viewing)}
          style={{
            padding: 0,
            display: 'inline-block',
            color: theme.onTextLight,
          }}
        >
          <ArrowLeft width='28px' height='28px' color={theme.onTextLight} />
        </Button>
        <Text color={theme.secondary}>
          {viewing.toLocaleDateString([], { year: 'numeric', month: 'long' })}
        </Text>
        <Button
          variant='text'
          onClick={viewNextMonth}
          style={{
            padding: 0,
            display: 'inline-block',
            color: theme.onTextLight,
          }}
        >
          <ArrowRight width='28px' height='28px' color={theme.onTextLight} />
        </Button>
      </div>
      {Object.values(t.common.weekdayShort).map(weekDay => (
        <Text
          key={`${weekDay}`}
          as='span'
          style={{ textTransform: 'uppercase' }}
          type='smallMedium'
        >
          {weekDay}
        </Text>
      ))}
      {calendar[0].map(week => (
        <>
          {week.map(day => {
            const isInMonth = inRange(
              day,
              startOfMonth(clearTime(viewing)),
              endOfMonth(clearTime(viewing)),
            )
            const selected = isSelected(day)
            const color = isInMonth
              ? selected
                ? theme.on
                : undefined
              : theme.placeholderText
            const disabled =
              !allowPast && clearTime(day) < clearTime(new Date())

            return (
              <Day
                data-selected={selected}
                key={`week-${week[0]}-day-${day}`}
                disabled={disabled || selected}
                onClick={() => {
                  toggle(day, true)
                  onChange(day)
                }}
                variant='text'
                selected={selected}
              >
                <Text color={color}>{day.getDate().toString()}</Text>
              </Day>
            )
          })}
        </>
      ))}
    </Box>
  )
}

export default DatePicker
