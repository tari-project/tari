import { useAppSelector, useAppDispatch } from '../../../store/hooks'
import {
  selectIsPending,
  selectIsRunning,
  selectWalletAddress,
} from '../../../store/wallet/selectors'
import { actions as walletActions } from '../../../store/wallet'
import PasswordPrompt from '../../../containers/PasswordPrompt'

import WalletSettings from './WalletSettings'

const WalletSettingsContainer = () => {
  const dispatch = useAppDispatch()
  const address = useAppSelector(selectWalletAddress)
  const running = useAppSelector(selectIsRunning)
  const pending = useAppSelector(selectIsPending)

  return (
    <PasswordPrompt local>
      <WalletSettings
        running={running}
        pending={pending}
        stop={() => dispatch(walletActions.stop())}
        start={() => dispatch(walletActions.start())}
        address={address}
      />
    </PasswordPrompt>
  )
}

export default WalletSettingsContainer
