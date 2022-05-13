import { useState } from 'react'
import { useTheme } from 'styled-components'

import { Schedule } from '../../../types/general'
import Text from '../../../components/Text'
import Box from '../../../components/Box'
import CalendarIcon from '../../../styles/Icons/Calendar'
import t from '../../../locales'
import { day } from '../../../utils/Format'

const WeekdaySelector = ({
  days,
  onChange,
}: {
  days: number[]
  onChange: (days: number[]) => void
}) => {
  const theme = useTheme()

  const toggle = (index: number) => {
    if (days.includes(index)) {
      onChange(days.filter(a => a !== index))
      return
    }

    onChange([...days, index])
  }

  return (
    <div
      style={{
        display: 'flex',
        justifyContent: 'space-between',
      }}
    >
      {Object.values(t.common.weekdayCapitals).map((capital, index) => {
        const color = days.includes(index)
          ? theme.accent
          : theme.placeholderText

        return (
          <div
            key={`${capital}${index}`}
            style={{
              width: '48px',
              height: '48px',
              backgroundColor: theme.backgroundImage,
              display: 'flex',
              justifyContent: 'center',
              alignItems: 'center',
              borderRadius: '4px',
              cursor: 'pointer',
            }}
            onClick={() => toggle(index)}
          >
            <Text color={color}>{capital}</Text>
          </div>
        )
      })}
    </div>
  )
}
WeekdaySelector.defaultProps = {
  days: [] as number[],
}

const DateScheduler = ({
  days,
  date,
  onChange,
}: {
  days?: number[]
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
      <div
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          marginTop: theme.spacing(),
        }}
      >
        {days && (
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
      </div>
    </Box>
  )
}

const ScheduleForm = ({
  value,
  onChange,
}: {
  value: Schedule
  onChange: (s: Schedule) => void
}) => {
  const [days, setDays] = useState(value?.days)
  const [date, setDate] = useState(value?.date)

  return (
    <DateScheduler
      days={days}
      date={date}
      onChange={({ days, date }) => {
        setDays(days?.sort((a, b) => a - b))
        setDate(date)
      }}
    />
  )
}

export default ScheduleForm
