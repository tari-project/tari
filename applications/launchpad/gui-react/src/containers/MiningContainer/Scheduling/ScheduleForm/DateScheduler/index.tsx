import { useTheme } from 'styled-components'

import Text from '../../../../../components/Text'
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

  const scheduleDays = (newDays: number[]) => {
    const d = [...newDays]
    d.sort((a, b) => a - b)
    onChange({
      days: d,
      date: undefined,
    })
  }

  const scheduleDate = (newDate: Date) => {
    onChange({
      date: newDate,
      days: undefined,
    })
  }

  return (
    <>
      <WeekdaySelector days={days} onChange={days => scheduleDays(days)} />
      <HumanReadableScheduledDate>
        <div>
          {!date && days && (
            <>
              <Text as='span' color={theme.secondary} type='smallMedium'>
                Every
              </Text>{' '}
              <Text as='span' type='smallMedium'>
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
            <Text as='span' type='smallMedium'>
              {day(date)}
            </Text>
          )}
        </div>
        <div
          onClick={() => scheduleDate(new Date())}
          style={{ cursor: 'pointer' }}
        >
          <CalendarIcon height='18px' width='18px' />
        </div>
      </HumanReadableScheduledDate>
    </>
  )
}
DateScheduler.defaultProps = {
  days: [],
}

export default DateScheduler
