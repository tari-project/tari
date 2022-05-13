import { useTheme } from 'styled-components'

import Box from '../../../../../components/Box'
import Text from '../../../../../components/Text'
import { day } from '../../../../../utils/Format'
import CalendarIcon from '../../../../../styles/Icons/Calendar'
import t from '../../../../../locales'
import WeekdaySelector from '../WeekdaySelector'

import { HumanReadableScheduledDate } from './styles'

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
    onChange({
      days: newDays,
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
    <Box border={false}>
      <WeekdaySelector days={days} onChange={days => scheduleDays(days)} />
      <HumanReadableScheduledDate>
        {!date && days && (
          <div>
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
          </div>
        )}
        {date && <Text type='smallMedium'>{day(date)}</Text>}
        <div
          onClick={() => scheduleDate(new Date())}
          style={{ cursor: 'pointer' }}
        >
          <CalendarIcon height='18px' width='18px' />
        </div>
      </HumanReadableScheduledDate>
    </Box>
  )
}
DateScheduler.defaultProps = {
  days: [],
}

export default DateScheduler
