import { useAppSelector } from '../../store/hooks'
import {
  selectWalletAddress,
  selectTariAmount,
  selectIsRunning,
  selectWalletEmojiAddress,
} from '../../store/wallet/selectors'

import TariWallet from './TariWallet'
import WalletBalance from './WalletBalance'

const WalletContent = () => {
  const walletAddress = useAppSelector(selectWalletAddress)
  const emojiId = useAppSelector(selectWalletEmojiAddress)
  const running = useAppSelector(selectIsRunning)
  const { balance, available } = useAppSelector(selectTariAmount)

  return (
    <>
      <TariWallet address={walletAddress} emojiId={emojiId} running={running} />
      <WalletBalance balance={balance} available={available} />
    </>
  )
}

export default WalletContent
