import { useState } from 'react'

import { useAppSelector } from '../../store/hooks'
import { selectExpertView } from '../../store/app/selectors'

import { CenteredLayout, ToTheLeftLayout } from './styles'
import PasswordBox from './PasswordBox'
import TariWallet from './TariWallet'
import WalletBalance from './WalletBalance'

const WalletContainer = () => {
  const [unlocked, setUnlocked] = useState(false)
  const expertView = useAppSelector(selectExpertView)

  const walletAddress = 'your tari wallet address'

  if (!unlocked) {
    return (
      <CenteredLayout>
        <PasswordBox onSubmit={() => setUnlocked(true)} />
      </CenteredLayout>
    )
  }

  return (
    <ToTheLeftLayout expertView={expertView}>
      <TariWallet address={walletAddress} />
      <WalletBalance balance={11350057} available={11349009} />
    </ToTheLeftLayout>
  )
}

export default WalletContainer
