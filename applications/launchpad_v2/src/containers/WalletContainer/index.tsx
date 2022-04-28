import { useAppSelector, useAppDispatch } from '../../store/hooks'
import { actions } from '../../store/wallet'
import {
  selectIsUnlocked,
  selectWalletAddress,
  selectTariAmount,
  selectIsPending,
} from '../../store/wallet/selectors'
import { selectExpertView } from '../../store/app/selectors'

import { CenteredLayout, ToTheLeftLayout } from './styles'
import PasswordBox from './PasswordBox'
import TariWallet from './TariWallet'
import WalletBalance from './WalletBalance'

const WalletContainer = () => {
  const dispatch = useAppDispatch()
  const expertView = useAppSelector(selectExpertView)
  const unlocked = useAppSelector(selectIsUnlocked)
  const walletAddress = useAppSelector(selectWalletAddress)
  const { balance, available } = useAppSelector(selectTariAmount)
  const pending = useAppSelector(selectIsPending)

  if (!unlocked) {
    return (
      <CenteredLayout>
        <PasswordBox
          pending={pending}
          onSubmit={password => dispatch(actions.unlockWallet(password))}
        />
      </CenteredLayout>
    )
  }

  return (
    <ToTheLeftLayout expertView={expertView}>
      <TariWallet address={walletAddress} />
      <WalletBalance balance={balance} available={available} />
    </ToTheLeftLayout>
  )
}

export default WalletContainer
