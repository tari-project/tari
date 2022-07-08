import { RootState } from '../'
import { Container } from '../containers/types'
import {
  selectRecipePending,
  selectRecipeRunning,
} from '../containers/selectors'

import { WalletSetupRequired } from './types'

export const selectIsUnlocked = (state: RootState) => state.wallet.unlocked

export const selectWalletAddress = (state: RootState) =>
  state.wallet.address.uri
export const selectWalletEmojiAddress = (state: RootState) =>
  state.wallet.address.emoji

export const selectTariBalance = (state: RootState) => state.wallet.tari

export const selectWalletSetupRequired = (state: RootState) =>
  !state.wallet.address.uri
    ? WalletSetupRequired.MissingWalletAddress
    : undefined

export const selectIsPending = selectRecipePending(Container.Wallet)

export const selectIsRunning = selectRecipeRunning(Container.Wallet)

export const selectLastTxHistoryUpdate = (state: RootState) =>
  state.wallet.lastTxHistoryUpdateAt
