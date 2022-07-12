import { useState, useCallback, KeyboardEvent } from 'react'
import { useTheme } from 'styled-components'

import Text from '../../../components/Text'
import Box from '../../../components/Box'
import Button from '../../../components/Button'
import { Schedule } from '../../../types/general'
import t from '../../../locales'

import { SchedulesListContainer, NoSchedulesContainer } from './styles'
import Actions from './Actions'
import SchedulePresentation from './Schedule'

type ScheduleId = string
type ScheduleActions = {
  toggle: (id: ScheduleId) => void
  edit: (id: ScheduleId) => void
  remove: (id: ScheduleId) => void
}

/**
 * @name ScheduleList
 * @description renders list of schedules and handles adding/editing/remove events
 *
 * @prop {Schedule[]} schedules - list of schedules to render
 * @prop {() => void} addSchedule - event called when user wants to add a new schedule
 * @prop {() => void} cancel - event called to close the list
 * @prop {(scheduleId: string) => void} toggle - event called to toggle selection of schedule
 * @prop {(scheduleId: string) => void} edit - event called to edit schedule
 * @prop {(scheduleId: string) => void} remove - event called to remove schedule
 */
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
      <Box
        border={false}
        style={{
          width: '100%',
          marginBottom: 0,
          padding: `${theme.spacing(1)} ${theme.spacing(2)}`,
          background: theme.nodeBackground,
        }}
      >
        <Text type='header'>{t.mining.scheduling.title}</Text>
        <Text
          as='p'
          style={{ marginTop: theme.spacing() }}
          color={theme.helpTipText}
        >
          {t.mining.scheduling.launchpadOpen}
        </Text>
      </Box>
      {schedules.length === 0 && (
        <NoSchedulesContainer>
          <Text
            as='p'
            style={{ marginBottom: theme.spacing() }}
            color={theme.primary}
          >
            {t.mining.scheduling.noSchedules}
          </Text>
          <Button onClick={addSchedule}>{t.mining.scheduling.add}</Button>
        </NoSchedulesContainer>
      )}
      {schedules.length !== 0 && (
        <SchedulesListContainer tabIndex={0} onKeyDown={onListKeyDown}>
          <Text
            type='smallMedium'
            color={theme.nodeWarningText}
            style={{
              alignSelf: 'flex-start',
              marginLeft: theme.spacingHorizontal(),
              marginBottom: theme.spacingVertical(0.5),
            }}
          >
            {t.mining.scheduling.doubleClick}
          </Text>
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
          {t.common.verbs.cancel}
        </Button>
        {schedules.length !== 0 && (
          <Button
            style={{ flexGrow: 2, justifyContent: 'center' }}
            onClick={addSchedule}
          >
            {t.mining.scheduling.add}
          </Button>
        )}
      </Actions>
    </>
  )
}

export default ScheduleList
