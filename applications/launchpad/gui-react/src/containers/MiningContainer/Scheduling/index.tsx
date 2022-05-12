import { useState } from 'react'

import Modal from '../../../components/Modal'
import { useAppSelector, useAppDispatch } from '../../../store/hooks'
import { selectSchedules } from '../../../store/app/selectors'
import { toggleSchedule } from '../../../store/app'

import ScheduleList from './ScheduleList'
import { ScheduleContainer } from './styles'

const SchedulingContainer = ({
  open,
  onClose,
}: {
  open: boolean
  onClose: () => void
}) => {
  const [addingSchedule, setAddingSchedule] = useState(false)
  const schedules = useAppSelector(selectSchedules)
  const dispatch = useAppDispatch()

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
            toggle={scheduleId => dispatch(toggleSchedule(scheduleId))}
            edit={() => null}
            remove={() => null}
          />
        )}
      </ScheduleContainer>
    </Modal>
  )
}

export default SchedulingContainer
