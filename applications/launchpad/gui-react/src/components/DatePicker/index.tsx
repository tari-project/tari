import { useEffect } from 'react'
import { useTheme } from 'styled-components'
import { useLilius } from 'use-lilius'

import ArrowLeft from '../../styles/Icons/ArrowLeft2'
import ArrowRight from '../../styles/Icons/ArrowRight2'
import t from '../../locales'
import Box from '../Box'
import Button from '../Button'
import Text from '../Text'

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

const isToday = (d: Date) => {
  const startOfToday = new Date()
  startOfToday.setHours(0)
  startOfToday.setMinutes(0)
  startOfToday.setSeconds(0)
  startOfToday.setMilliseconds(0)

  const endOfToday = new Date()
  endOfToday.setHours(23)
  endOfToday.setMinutes(59)
  endOfToday.setSeconds(59)
  endOfToday.setMilliseconds(999)

  return d >= startOfToday && d <= endOfToday
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
  const {
    calendar,
    clearSelected,
    clearTime,
    inRange,
    isSelected,
    select,
    setViewing,
    toggle,
    viewing,
    viewNextMonth,
    viewPreviousMonth,
  } = useLilius()
  const theme = useTheme()
  useEffect(() => {
    if (value) {
      select(value)
      setViewing(value)

      return
    }

    clearSelected()
  }, [value, select, clearSelected])

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
        columnGap: theme.spacing(0.5),
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
          <ArrowLeft width='20px' height='20px' color={theme.onTextLight} />
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
          <ArrowRight width='20px' height='20px' color={theme.onTextLight} />
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
            const color = isInMonth ? undefined : theme.placeholderText
            const disabled =
              !allowPast && clearTime(day) < clearTime(new Date())

            return (
              <Button
                data-selected={isSelected(day)}
                key={`week-${week[0]}-day-${day}`}
                onClick={
                  disabled
                    ? undefined
                    : () => {
                        toggle(day, true)
                        onChange(day)
                      }
                }
                variant='text'
                style={{
                  padding: theme.spacing(0.25),
                  backgroundColor: isSelected(day) ? theme.onTextLight : '',
                  color: disabled
                    ? theme.placeholderText
                    : isSelected(day)
                    ? theme.on
                    : theme.primary,
                  borderRadius: '50%',
                  textDecoration: isToday(day) ? 'underline' : '',
                }}
              >
                <Text color={color}>{day.getDate().toString()}</Text>
              </Button>
            )
          })}
        </>
      ))}
    </Box>
  )
}

export default DatePicker
