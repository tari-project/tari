import { Fragment } from 'react'
import { useTheme } from 'styled-components'
import { useLilius } from 'use-lilius'

import ArrowLeft from '../../styles/Icons/ArrowLeft2'
import ArrowRight from '../../styles/Icons/ArrowRight2'
import t from '../../locales'
import { month } from '../../utils/Format'
import {
  startOfDay,
  endOfMonth,
  startOfMonth,
  isCurrentMonth,
} from '../../utils/Date'
import Button from '../Button'
import Text from '../Text'

import Day from './Day'
import DatePickerWrapper from './DatePickerWrapper'
import { MonthContainer } from './styles'
import { DatePickerProps } from './types'

const allowPast = false

/**
 * @name DatePickerComponent
 * @description date picker component that renders calendar and returns selected date
 *
 * @prop {Date} [value] - selected value
 * @prop {(d: Date) => void} onChange - callback called when user selects a date
 * style {CSSProperties} [style] - optional styles to main container of the date picker
 */
const DatePickerComponent = ({
  value,
  onChange,
  style,
}: Omit<DatePickerProps, 'open'>) => {
  const theme = useTheme()
  const valueWithoutTime = value && startOfDay(value)

  const {
    calendar,
    inRange,
    isSelected,
    toggle,
    viewing,
    viewNextMonth,
    viewPreviousMonth,
  } = useLilius({
    viewing: valueWithoutTime,
    selected: valueWithoutTime ? [valueWithoutTime] : [],
  })

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
        <Text color={theme.calendarText}>{month(viewing)}</Text>
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
          color={theme.calendarTextSecondary}
        >
          {weekDay}
        </Text>
      ))}
      {calendar[0].map((week, weekId) => (
        <Fragment key={`week-${weekId}`}>
          {week.map(day => {
            const isInMonth = inRange(
              day,
              startOfMonth(viewing),
              endOfMonth(viewing),
            )
            const labelColor = isInMonth
              ? isSelected(day)
                ? theme.on
                : undefined
              : theme.disabledPrimaryButtonText
            const disabled =
              !allowPast && startOfDay(day) < startOfDay(new Date())

            return (
              <Day
                data-selected={isSelected(day)}
                key={`week-${weekId}-day-${day.getDay()}`}
                disabled={disabled || isSelected(day)}
                onClick={() => {
                  toggle(day, true)
                  onChange(
                    new Date(
                      Date.UTC(
                        day.getFullYear(),
                        day.getMonth(),
                        day.getDate(),
                      ),
                    ),
                  )
                }}
                variant='text'
                selected={isSelected(day)}
              >
                <Text color={labelColor}>{day.getDate().toString()}</Text>
              </Day>
            )
          })}
        </Fragment>
      ))}
    </DatePickerWrapper>
  )
}

export default DatePickerComponent
