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
  const populate = () => {
    dispatch(actions.addNotification({ amount: 1232, currency: 'xtr' }))
    dispatch(actions.addNotification({ amount: 2344, currency: 'xtr' }))
  }

  return (
    <>
      <button onClick={populate}>test</button>
      {notification ? (
        <DelayRender
          render={() => (
            <TariNotification
              open={open}
              notification={notification}
              onClose={onClose}
            />
          )}
        />
      ) : null}
    </>
  )
}

export default TariNotificationContainer
