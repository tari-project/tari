import { useState, useCallback } from 'react'
import { useTheme } from 'styled-components'

import { Schedule, Interval } from '../../../../types/general'
import Button from '../../../../components/Button'
import Box from '../../../../components/Box'
import t from '../../../../locales'
import Actions from '../Actions'

import DateScheduler from './DateScheduler'
import MiningTypeSelector from './MiningTypeSelector'
import RemoveSchedule from './RemoveSchedule'
import IntervalPicker from './IntervalPicker'
import ScheduleFormError from './ScheduleFormError'
import { validate } from './validation'

/**
 * @name ScheduleForm
 * @description renders add/edit form for Schedule in mining scheduling
 *
 * @prop {Schedule} [value] - initial values of schedule being edited
 * @prop {() => void} cancel - callback for user action of cancelling editing
 * @prop {() => void} remove - callback for user action of removing edited schedule
 * @prop {(s: Schedule) => void} onChange - called with new Schedule after user accepts changes
 */
const ScheduleForm = ({
  value,
  cancel,
  onChange,
  remove,
}: {
  value?: Schedule
  cancel: () => void
  remove: () => void
  onChange: (s: Schedule) => void
}) => {
  const editing = Boolean(value)
  const theme = useTheme()
  const [error, setError] = useState<string | undefined>()
  const [days, setDays] = useState(value?.days)
  const [date, setDate] = useState(value?.date)
  const [miningType, setMiningType] = useState(value?.type || [])
  const [interval, setInterval] = useState(
    value?.interval ||
      ({
        from: { hours: 0, minutes: 0 },
        to: { hours: 0, minutes: 0 },
      } as Interval),
  )

  const enableSave =
    ((days?.length || 0) > 0 || date) && (miningType.length || 0) > 0

  const updateSchedule = useCallback(() => {
    const updatedSchedule: Schedule = {
      id: value?.id || Date.now().toString(),
      enabled: value ? value.enabled : true,
      days,
      date,
      interval,
      type: miningType,
    }

    const error = validate(updatedSchedule)
    if (error) {
      setError(error)

      return
    }

    onChange(updatedSchedule)
  }, [value, days, date, interval, miningType])

  return (
    <>
      <Box
        border={false}
        style={{
          rowGap: theme.spacing(),
          display: 'flex',
          flexDirection: 'column',
          width: '100%',
          padding: `${theme.spacing()} ${theme.spacing(1.5)}`,
          paddingBottom: 0,
          marginBottom: 0,
        }}
      >
        <MiningTypeSelector value={miningType} onChange={setMiningType} />
        <DateScheduler
          days={days}
          date={date}
          onChange={({ days, date }) => {
            setDays(days?.sort((a, b) => a - b))
            setDate(date)
          }}
        />
        <IntervalPicker value={interval} onChange={setInterval} />
        {editing && <RemoveSchedule remove={remove} />}
      </Box>
      <Actions>
        <Button variant='secondary' onClick={cancel}>
          {t.common.verbs.cancel}
        </Button>
        <Button
          style={{ flexGrow: 2, justifyContent: 'center' }}
          onClick={updateSchedule}
          disabled={!enableSave}
        >
          {t.common.verbs.save}
        </Button>
      </Actions>
      <ScheduleFormError
        error={error}
        clearError={() => setError(undefined)}
        cancel={cancel}
      />
    </>
  )
}

export default ScheduleForm
