import { useState } from 'react'
import { useTheme } from 'styled-components'

import Text from '../../../../../components/Text'
import Button from '../../../../../components/Button'
import DatePicker from '../../../../../components/DatePicker'
import { day } from '../../../../../utils/Format'
import CalendarIcon from '../../../../../styles/Icons/Calendar'
import t from '../../../../../locales'
import WeekdaySelector from '../WeekdaySelector'

import { HumanReadableScheduledDate } from './styles'

/**
 * @name DateScheduler
 * @description Allows to select schedule - either a specific date or week days for recurring schedules
 *
 * @prop {number[]} [days] - selected days (where 0 - Sunday, 1 - Monday etc.)
 * @prop {Date} [date] - selected date
 * @prop {(schedule: { days?: number[]; date?: Date }) => void} onChange - called with either selected days array or selected day
 */
const DateScheduler = ({
  days,
  date,
  onChange,
}: {
  days: number[]
  date?: Date
  onChange: (schedule: { days?: number[]; date?: Date }) => void
}) => {
  const theme = useTheme()
  const [calendarOpen, setCalendarOpen] = useState(false)

  const toggleCalendar = () => setCalendarOpen(s => !s)

  const scheduleDays = (newDays: number[]) => {
    const d = [...newDays]
    d.sort((a, b) => a - b)
    onChange({
      days: d,
      date: undefined,
    })
    setCalendarOpen(false)
  }

  const scheduleDate = (newDate: Date) => {
    onChange({
      date: newDate,
      days: undefined,
    })
    setCalendarOpen(false)
  }

  return (
    <>
      <WeekdaySelector days={days} onChange={days => scheduleDays(days)} />
      <HumanReadableScheduledDate>
        <div>
          {!date && days && (
            <>
              <Text as='span' color={theme.nodeWarningText} type='smallMedium'>
                Every
              </Text>{' '}
              <Text as='span' type='smallMedium' color={theme.primary}>
                {days &&
                  days
                    .map(
                      selectedDay =>
                        Object.values(t.common.weekdayShort)[selectedDay],
                    )
                    .join(', ')}
              </Text>
            </>
          )}
          {date && (
            <Button
              variant='button-in-text'
              onClick={toggleCalendar}
              style={{ textDecoration: 'none' }}
            >
              <Text as='span' type='smallMedium'>
                {day(date)}
              </Text>
            </Button>
          )}
        </div>
        <div onClick={toggleCalendar} style={{ cursor: 'pointer' }}>
          <CalendarIcon
            height='18px'
            width='18px'
            color={theme.nodeWarningText}
          />
        </div>
      </HumanReadableScheduledDate>
      <DatePicker
        value={date}
        open={calendarOpen}
        onChange={scheduleDate}
        style={{
          position: 'absolute',
          left: '100%',
          width: 'auto',
          minWidth: 0,
          marginTop: `-${theme.spacing(2)}`,
          marginLeft: theme.spacing(0.25),
        }}
      />
    </>
  )
}
DateScheduler.defaultProps = {
  days: [],
}

export default DateScheduler
