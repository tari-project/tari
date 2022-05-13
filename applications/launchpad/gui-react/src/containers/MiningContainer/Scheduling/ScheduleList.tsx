import { useState, useCallback, KeyboardEvent } from 'react'
import { useTheme } from 'styled-components'

import Text from '../../../components/Text'
import Box from '../../../components/Box'
import Button from '../../../components/Button'
import { Schedule } from '../../../types/general'

import { SchedulesListContainer, NoSchedulesContainer } from './styles'
import Actions from './Actions'
import SchedulePresentation from './Schedule'

type ScheduleId = string
type ScheduleActions = {
  toggle: (id: ScheduleId) => void
  edit: (id: ScheduleId) => void
  remove: (id: ScheduleId) => void
}

const ScheduleList = ({
  schedules,
  addSchedule,
  cancel,
  toggle,
  edit,
  remove,
}: {
  schedules: Schedule[]
  cancel: () => void
  addSchedule: () => void
} & ScheduleActions) => {
  const theme = useTheme()
  const [selectedSchedule, setSelected] = useState('')

  const onListKeyDown = useCallback(
    (event: KeyboardEvent<HTMLElement>) => {
      const { key } = event
      if (['Delete', 'Backspace'].includes(key)) {
        remove(selectedSchedule)
      }
    },
    [remove, selectedSchedule],
  )

  return (
    <>
      <Box border={false} style={{ width: '100%', marginBottom: 0 }}>
        <Text type='header'>Mining schedules</Text>
        <Text as='p' style={{ marginTop: theme.spacing() }}>
          Tari Launchpad must be open at the scheduled hours for mining to
          start.
        </Text>
      </Box>
      {schedules.length === 0 && (
        <NoSchedulesContainer>
          <Text as='p' style={{ marginBottom: theme.spacing() }}>
            No mining schedule has been set up yet
          </Text>
          <Button onClick={addSchedule}>Add schedule</Button>
        </NoSchedulesContainer>
      )}
      {schedules.length !== 0 && (
        <SchedulesListContainer tabIndex={0} onKeyDown={onListKeyDown}>
          {schedules.map(schedule => (
            <SchedulePresentation
              key={schedule.id}
              {...schedule}
              toggle={() => toggle(schedule.id)}
              edit={() => edit(schedule.id)}
              selected={selectedSchedule === schedule.id}
              select={() => setSelected(schedule.id)}
            />
          ))}
        </SchedulesListContainer>
      )}
      <Actions>
        <Button variant='secondary' onClick={cancel}>
          Cancel
        </Button>
        {schedules.length !== 0 && (
          <Button
            style={{ flexGrow: 2, justifyContent: 'center' }}
            onClick={addSchedule}
          >
            Add schedule
          </Button>
        )}
      </Actions>
    </>
  )
}

export default ScheduleList
