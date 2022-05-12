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
  const schedules: Schedule[] = []

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
          />
        )}
      </ScheduleContainer>
    </Modal>
  )
}

export default SchedulingContainer
