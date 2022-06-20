import { useState, useEffect } from 'react'
import { useAppSelector, useAppDispatch } from '../../../store/hooks'
import { actions } from '../../../store/mining'
import { selectNotifications } from '../../../store/mining/selectors'

import TariNotification from './TariNotificationComponent'
import DelayRender from './DelayRender'

const TariNotificationContainer = () => {
  const [notification] = useAppSelector(selectNotifications)
  const dispatch = useAppDispatch()
  const [open, setOpen] = useState(true)
  const onClose = () => {
    setOpen(false)
    dispatch(actions.acknowledgeNotification())
  }
  useEffect(() => setOpen(true), [notification])

  return notification ? (
    <DelayRender
      render={() => (
        <TariNotification
          open={open}
          notification={notification}
          onClose={onClose}
        />
      )}
    />
  ) : null
}

export default TariNotificationContainer
