import { useAppSelector, useAppDispatch } from '../../store/hooks'
import { actions } from '../../store/wallet'
import {
  selectIsUnlocked,
  selectIsPending,
  selectWalletSetupRequired,
} from '../../store/wallet/selectors'
import { WalletSetupRequired } from '../../store/wallet/types'
import CenteredLayout from '../../components/CenteredLayout'

import WalletContent from './WalletContent'
import PasswordBox from './PasswordBox'
import WalletSetupBox from './WalletSetupBox'

const WalletContainer = () => {
  const dispatch = useAppDispatch()
  const unlocked = useAppSelector(selectIsUnlocked)
  const pending = useAppSelector(selectIsPending)
  const walletSetupRequired = useAppSelector(selectWalletSetupRequired)

  if (walletSetupRequired === WalletSetupRequired.MissingWalletAddress) {
    return (
      <CenteredLayout horizontally>
        <WalletSetupBox />
      </CenteredLayout>
    )
  }

  if (!unlocked) {
    return (
      <CenteredLayout horizontally vertically>
        <PasswordBox
          pending={pending}
          onSubmit={password => dispatch(actions.unlockWallet(password))}
        />
      </CenteredLayout>
    )
  }

  return (
    <CenteredLayout horizontally>
      <WalletContent />
    </CenteredLayout>
  )
}

export default WalletContainer
