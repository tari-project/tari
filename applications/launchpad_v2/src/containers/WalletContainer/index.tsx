import { useAppSelector, useAppDispatch } from '../../store/hooks'
import { actions } from '../../store/wallet'
import {
  selectIsUnlocked,
  selectWalletAddress,
  selectTariAmount,
  selectIsPending,
  selectIsRunning,
  selectWalletEmojiAddress,
} from '../../store/wallet/selectors'
import CenteredLayout from '../../components/CenteredLayout'

import PasswordBox from './PasswordBox'
import TariWallet from './TariWallet'
import WalletBalance from './WalletBalance'

const WalletContainer = () => {
  const dispatch = useAppDispatch()
  const unlocked = useAppSelector(selectIsUnlocked)
  const walletAddress = useAppSelector(selectWalletAddress)
  const emojiId = useAppSelector(selectWalletEmojiAddress)
  const { balance, available } = useAppSelector(selectTariAmount)
  const pending = useAppSelector(selectIsPending)
  const running = useAppSelector(selectIsRunning)

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
      <TariWallet address={walletAddress} emojiId={emojiId} running={running} />
      <WalletBalance balance={balance} available={available} />
    </CenteredLayout>
  )
}

export default WalletContainer
