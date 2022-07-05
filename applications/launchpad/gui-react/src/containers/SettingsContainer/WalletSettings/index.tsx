import { useAppSelector, useAppDispatch } from '../../../store/hooks'
import {
  selectIsPending,
  selectIsRunning,
  selectWalletAddress,
} from '../../../store/wallet/selectors'
import { actions as walletActions } from '../../../store/wallet'
import { WalletPasswordPrompt } from '../../../useWithWalletPassword'

import WalletSettings from './WalletSettings'

const WalletSettingsContainer = () => {
  const dispatch = useAppDispatch()
  const address = useAppSelector(selectWalletAddress)
  const running = useAppSelector(selectIsRunning)
  const pending = useAppSelector(selectIsPending)

  return (
    <WalletPasswordPrompt local>
      <WalletSettings
        running={running}
        pending={pending}
        stop={() => dispatch(walletActions.stop())}
        start={() => dispatch(walletActions.start())}
        address={address}
      />
    </WalletPasswordPrompt>
  )
}

export default WalletSettingsContainer
