import { useState } from 'react'

import TariNotification from './TariNotificationComponent'

const TariNotificationContainer = () => {
  const [open, setOpen] = useState(true)
  const amount = 999
  const onClose = () => setOpen(false)

  return open ? <TariNotification amount={amount} onClose={onClose} /> : null
}

export default TariNotificationContainer
