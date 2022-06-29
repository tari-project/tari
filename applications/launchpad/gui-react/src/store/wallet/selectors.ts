import { RootState } from '../'
import { Container } from '../containers/types'
import {
  selectRecipePending,
  selectRecipeRunning,
} from '../containers/selectors'

import { WalletSetupRequired } from './types'

export const selectState = (state: RootState) => state.wallet
export const selectIsUnlocked = (state: RootState) => state.wallet.unlocked
export const selectWalletAddress = (state: RootState) => state.wallet.address
export const selectWalletEmojiAddress = () => [
  '🐎🍴🌷',
  '🌟💻🐖',
  '🐩🐾🌟',
  '🐬🎧🐌',
  '🏦🐳🐎',
  '🐝🐢🔋',
  '👕🎸👿',
  '🍒🐓🎉',
  '💔🌹🏆',
  '🐬💡🎳',
  '🚦🍹🎒',
]
export const selectTariBalance = (state: RootState) => state.wallet.tari

export const selectWalletSetupRequired = (state: RootState) =>
  !state.wallet.address ? WalletSetupRequired.MissingWalletAddress : undefined

export const selectIsPending = selectRecipePending(Container.Wallet)

export const selectIsRunning = selectRecipeRunning(Container.Wallet)
