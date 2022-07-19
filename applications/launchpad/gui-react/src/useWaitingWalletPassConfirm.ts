import { useEffect, useState } from 'react'

import { AppDispatch } from './store'
import { useAppSelector } from './store/hooks'
import { selectWalletContainerLastAction } from './store/wallet/selectors'
import { selectWalletPasswordConfirmation } from './store/temporary/selectors'
import { temporaryActions } from './store/temporary'
import { SystemEventAction } from './store/containers/types'

export const useWaitingWalletPassConfirm = ({
  dispatch,
}: {
  dispatch: AppDispatch
}) => {
  const walletLastState = useAppSelector(selectWalletContainerLastAction)
  const walletPassConfirm = useAppSelector(selectWalletPasswordConfirmation)

  const [counter, setCounter] = useState(0)

  useEffect(() => {
    const clockVal = counter + 1
    if (walletPassConfirm === 'waiting_for_confirmation') {
      setTimeout(() => {
        setCounter(clockVal)
      }, 1000)
    } else {
      setCounter(0)
    }
  }, [counter, walletPassConfirm])

  useEffect(() => {
    if (walletPassConfirm !== 'waiting_for_confirmation') {
      return
    }

    if (counter > 20) {
      dispatch(temporaryActions.setWalletPasswordConfirmation('success'))
      setCounter(0)
    }

    if (counter > 15 && Boolean(walletLastState.error)) {
      dispatch(temporaryActions.setWalletPasswordConfirmation('failed'))
      setCounter(0)
    }

    if (
      counter > 3 &&
      (walletLastState?.status === SystemEventAction.Die ||
        walletLastState?.status === SystemEventAction.Destroy)
    ) {
      if (walletLastState?.exitCode === 13) {
        dispatch(
          temporaryActions.setWalletPasswordConfirmation('wrong_password'),
        )
      } else {
        if (counter > 5) {
          dispatch(temporaryActions.setWalletPasswordConfirmation('failed'))
        }
      }
    }
  }, [counter, walletLastState])
}
