import { useAppSelector, useAppDispatch } from '../../../store/hooks'
import {
  selectIsPending,
  selectIsRunning,
  selectState,
} from '../../../store/wallet/selectors'
import { actions as walletActions } from '../../../store/wallet'

import WalletSettings from './WalletSettings'

const WalletSettingsContainer = () => {
  const dispatch = useAppDispatch()
  const { address } = useAppSelector(selectState)
  const running = useAppSelector(selectIsRunning)
  const pending = useAppSelector(selectIsPending)

  return (
    <WalletSettings
      running={running}
      pending={pending}
      stop={() => dispatch(walletActions.stop())}
      start={password => dispatch(walletActions.start(password))}
      address={address}
    />
  )
}

export default WalletSettingsContainer
