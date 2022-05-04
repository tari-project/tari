import { useAppSelector, useAppDispatch } from '../../../store/hooks'
import { selectState as selectWalletState } from '../../../store/wallet/selectors'
import { actions as walletActions } from '../../../store/wallet'

import WalletSettings from './WalletSettings'

const WalletSettingsContainer = () => {
  const dispatch = useAppDispatch()
  const { pending, running, address } = useAppSelector(selectWalletState)

  return (
    <WalletSettings
      running={running}
      pending={pending}
      stop={() => dispatch(walletActions.stop())}
      start={() => dispatch(walletActions.start())}
      address={address}
    />
  )
}

export default WalletSettingsContainer
