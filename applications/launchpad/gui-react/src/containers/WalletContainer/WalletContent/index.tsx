import { useAppSelector } from '../../../store/hooks'
import CenteredLayout from '../../../components/CenteredLayout'
import {
  selectIsRunning,
  selectWalletEmojiAddress,
  selectWalletPublicKey,
} from '../../../store/wallet/selectors'
import TariWallet from '../TariWallet'
import WalletBalance from '../WalletBalance'
import WalletHelp from '../WalletHelp'

import useWalletBalance from './useWalletBalance'
import RecentTransactions from '../RecentTransactions'

const WalletContent = () => {
  const walletAddress = useAppSelector(selectWalletPublicKey)
  const emojiId = useAppSelector(selectWalletEmojiAddress)
  const running = useAppSelector(selectIsRunning)
  const { balance, available, pending: balancePending } = useWalletBalance()

  return (
    <div>
      <WalletHelp header />
      <CenteredLayout>
        <TariWallet
          address={walletAddress}
          emojiId={emojiId}
          running={running}
        />
        <WalletBalance
          balance={balance}
          available={available}
          pending={balancePending}
        />
      </CenteredLayout>
      <RecentTransactions />
    </div>
  )
}

export default WalletContent
