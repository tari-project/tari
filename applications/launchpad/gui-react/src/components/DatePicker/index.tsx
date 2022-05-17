import { useEffect, CSSProperties } from 'react'
import { useTheme } from 'styled-components'
import { useLilius } from 'use-lilius'

import ArrowLeft from '../../styles/Icons/ArrowLeft2'
import ArrowRight from '../../styles/Icons/ArrowRight2'
import t from '../../locales'
import { month } from '../../utils/Format'
import Button from '../Button'
import Text from '../Text'

import Day from './Day'
import DatePickerWrapper from './DatePickerWrapper'
import { MonthContainer } from './styles'
import { endOfMonth, startOfMonth, isCurrentMonth } from './utils'

const allowPast = false

const DatePicker = ({
  open,
  value,
  onChange,
  style,
}: {
  open: boolean
  value?: Date
  onChange: (d: Date) => void
  style?: CSSProperties
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
    <DatePickerWrapper style={style}>
      <MonthContainer>
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
        <Text color={theme.secondary}>{month(viewing)}</Text>
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
      </MonthContainer>
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
            const labelColor = isInMonth
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
                <Text color={labelColor}>{day.getDate().toString()}</Text>
              </Day>
            )
          })}
        </>
      ))}
    </DatePickerWrapper>
  )
}

export default DatePicker
