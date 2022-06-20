import { useAppSelector, useAppDispatch } from '../../store/hooks'
import { actions } from '../../store/wallet'
import { selectIsUnlocked, selectIsPending } from '../../store/wallet/selectors'
import CenteredLayout from '../../components/CenteredLayout'
import { selectTariSetupRequired } from '../../store/mining/selectors'
import { TariMiningSetupRequired } from '../../store/mining/types'

import WalletContent from './WalletContent'
import PasswordBox from './PasswordBox'
import WalletSetupBox from './WalletSetupBox'

const WalletContainer = () => {
  const dispatch = useAppDispatch()
  const unlocked = useAppSelector(selectIsUnlocked)
  const pending = useAppSelector(selectIsPending)
  const tariSetupRequired = useAppSelector(selectTariSetupRequired)

  if (tariSetupRequired === TariMiningSetupRequired.MissingWalletAddress) {
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
