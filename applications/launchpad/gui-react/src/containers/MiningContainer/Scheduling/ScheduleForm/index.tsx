import { useState } from 'react'

import { Schedule } from '../../../../types/general'
import Button from '../../../../components/Button'
import t from '../../../../locales'
import Actions from '../Actions'

import DateScheduler from './DateScheduler'

const ScheduleForm = ({
  value,
  cancel,
  onChange,
}: {
  value: Schedule
  cancel: () => void
  onChange: (s: Schedule) => void
}) => {
  const [days, setDays] = useState(value?.days)
  const [date, setDate] = useState(value?.date)

  const updateSchedule = () => {
    // do things
  }

  return (
    <>
      <DateScheduler
        days={days}
        date={date}
        onChange={({ days, date }) => {
          setDays(days?.sort((a, b) => a - b))
          setDate(date)
        }}
      />
      <Actions>
        <Button variant='secondary' onClick={cancel}>
          {t.common.verbs.cancel}
        </Button>
        <Button
          style={{ flexGrow: 2, justifyContent: 'center' }}
          onClick={updateSchedule}
        >
          {t.common.verbs.save}
        </Button>
      </Actions>
    </>
  )
}

export default ScheduleForm
