import { useState } from 'react'

import Modal from '../../../components/Modal'

import ScheduleList from './ScheduleList'
import { Schedule } from './types'
import { ScheduleContainer } from './styles'

const SchedulingContainer = ({
  open,
  onClose,
}: {
  open: boolean
  onClose: () => void
}) => {
  const [addingSchedule, setAddingSchedule] = useState(false)
  const schedules: Schedule[] = [
    {
      id: 'asdf',
      enabled: true,
      days: [0, 1, 2],
      interval: {
        from: { hours: 3, minutes: 0 },
        to: { hours: 19, minutes: 35 },
      },
      type: ['merged'],
    },
    {
      id: 'qwer',
      enabled: false,
      days: [4, 5],
      interval: {
        from: { hours: 7, minutes: 0 },
        to: { hours: 15, minutes: 0 },
      },
      type: ['merged', 'tari'],
    },
    {
      id: 'qwer1',
      enabled: true,
      date: new Date('2022-05-14'),
      interval: {
        from: { hours: 7, minutes: 0 },
        to: { hours: 15, minutes: 0 },
      },
      type: ['merged', 'tari'],
    },
    {
      id: 'qwer3',
      enabled: false,
      date: new Date('2022-05-14'),
      interval: {
        from: { hours: 7, minutes: 0 },
        to: { hours: 15, minutes: 0 },
      },
      type: ['merged', 'tari'],
    },
  ]

  const close = () => {
    setAddingSchedule(false)
    onClose()
  }

  return (
    <Modal open={open} onClose={close} size='small'>
      <ScheduleContainer>
        {!addingSchedule && (
          <ScheduleList
            schedules={schedules}
            cancel={close}
            addSchedule={() => setAddingSchedule(true)}
            toggle={() => null}
            edit={() => null}
            remove={() => null}
          />
        )}
      </ScheduleContainer>
    </Modal>
  )
}

export default SchedulingContainer
