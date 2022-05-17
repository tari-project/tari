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
import { clearTime, endOfMonth, startOfMonth, isCurrentMonth } from './utils'
import { DatePickerProps } from './types'

const allowPast = false

const DatePicker = ({
  value,
  onChange,
  style,
}: Omit<DatePickerProps, 'open'>) => {
  const theme = useTheme()
  const valueWithoutTime = value && clearTime(value)

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
            const labelColor = isInMonth
              ? isSelected(day)
                ? theme.on
                : undefined
              : theme.placeholderText
            const disabled =
              !allowPast && clearTime(day) < clearTime(new Date())

            return (
              <Day
                data-selected={isSelected(day)}
                key={`week-${week[0]}-day-${day}`}
                disabled={disabled || isSelected(day)}
                onClick={() => {
                  toggle(day, true)
                  onChange(day)
                }}
                variant='text'
                selected={isSelected(day)}
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
