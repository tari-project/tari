import { useAppSelector, useAppDispatch } from '../../../store/hooks'
import { actions } from '../../../store/mining'
import { selectNotifications } from '../../../store/mining/selectors'

import TariNotification from './TariNotificationComponent'
import DelayRender from './DelayRender'

const TariNotificationContainer = () => {
  const [notification] = useAppSelector(selectNotifications)
  const dispatch = useAppDispatch()
  const onClose = () => dispatch(actions.acknowledgeNotification())
  const populate = () => dispatch(actions.addDummyNotification())

  return (
    <>
      <button onClick={populate}>test</button>
      {notification ? (
        <DelayRender
          render={() => (
            <TariNotification amount={notification.amount} onClose={onClose} />
          )}
        />
      ) : null}
    </>
  )
}

export default TariNotificationContainer
