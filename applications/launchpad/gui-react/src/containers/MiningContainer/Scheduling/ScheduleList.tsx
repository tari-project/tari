import { useTheme } from 'styled-components'

import Text from '../../../components/Text'
import Box from '../../../components/Box'
import Button from '../../../components/Button'

import { Schedule } from './types'
import { NoSchedulesContainer, Actions } from './styles'

const ScheduleList = ({
  schedules,
  addSchedule,
  cancel,
}: {
  schedules: Schedule[]
  cancel: () => void
  addSchedule: () => void
}) => {
  const theme = useTheme()

  return (
    <>
      <Box border={false} style={{ width: '100%' }}>
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
      <Actions>
        <Button variant='secondary' onClick={cancel}>
          Cancel
        </Button>
      </Actions>
    </>
  )
}

export default ScheduleList
