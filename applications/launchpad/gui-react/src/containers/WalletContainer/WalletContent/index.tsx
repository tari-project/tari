import { useAppSelector } from '../../../store/hooks'
import {
  selectWalletAddress,
  selectIsRunning,
  selectWalletEmojiAddress,
} from '../../../store/wallet/selectors'
import TariWallet from '../TariWallet'
import WalletBalance from '../WalletBalance'

import useWalletBalance from './useWalletBalance'

const WalletContent = () => {
  const walletAddress = useAppSelector(selectWalletAddress)
  const emojiId = useAppSelector(selectWalletEmojiAddress)
  const running = useAppSelector(selectIsRunning)
  const { balance, available, pending: balancePending } = useWalletBalance()

  return (
    <>
      <TariWallet address={walletAddress} emojiId={emojiId} running={running} />
      <WalletBalance
        balance={balance}
        available={available}
        pending={balancePending}
      />
    </>
  )
}

export default WalletContent
