import { useAppSelector, useAppDispatch } from '../../../store/hooks'
import {
  selectIsPending,
  selectIsRunning,
  selectState,
} from '../../../store/wallet/selectors'
import { actions as walletActions } from '../../../store/wallet'

import WalletSettings from './WalletSettings'
import { WalletPasswordPrompt } from './useWithWalletPassword'

const WalletSettingsContainer = () => {
  const dispatch = useAppDispatch()
  const { address } = useAppSelector(selectState)
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
