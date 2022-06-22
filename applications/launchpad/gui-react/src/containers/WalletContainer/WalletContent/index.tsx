import { useAppSelector } from '../../../store/hooks'
import CenteredLayout from '../../../components/CenteredLayout'
import {
  selectWalletAddress,
  selectIsRunning,
  selectWalletEmojiAddress,
} from '../../../store/wallet/selectors'
import TariWallet from '../TariWallet'
import WalletBalance from '../WalletBalance'
import WalletHelp from '../WalletHelp'

import useWalletBalance from './useWalletBalance'

const WalletContent = () => {
  const walletAddress = useAppSelector(selectWalletAddress)
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
    </div>
  )
}

export default WalletContent
