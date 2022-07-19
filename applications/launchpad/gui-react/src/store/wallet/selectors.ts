import { RootState } from '../'
import { Container } from '../containers/types'
import {
  selectContainer,
  selectContainerError,
  selectRecipePending,
  selectRecipeRunning,
} from '../containers/selectors'

import { WalletSetupRequired } from './types'
import { createSelector } from '@reduxjs/toolkit'

export const selectIsUnlocked = (state: RootState) => state.wallet.unlocked

export const selectWalletAddress = (state: RootState) =>
  state.wallet.address.uri
export const selectWalletEmojiAddress = (state: RootState) =>
  state.wallet.address.emoji
export const selectWalletPublicKey = (state: RootState) =>
  state.wallet.address.publicKey

export const selectTariBalance = (state: RootState) => state.wallet.tari

export const selectWalletSetupRequired = (state: RootState) =>
  !state.wallet.address.uri
    ? WalletSetupRequired.MissingWalletAddress
    : undefined

export const selectIsPending = selectRecipePending(Container.Wallet)

export const selectIsRunning = selectRecipeRunning(Container.Wallet)

export const selectLastTxHistoryUpdate = (state: RootState) =>
  state.wallet.lastTxHistoryUpdateAt

export const selectWalletContainerLastAction = createSelector(
  selectContainer('wallet'),
  selectContainerError('wallet'),
  (walletContainer, walletError) => ({
    status: walletContainer.containerStatus?.status,
    exitCode: walletContainer.containerStatus?.exitCode,
    error: walletError,
  }),
  {
    memoizeOptions: {
      equalityCheck: (a, b) => {
        if (a && typeof a !== 'string' && 'containerStatus' in a) {
          return (
            a.containerStatus?.status === b.containerStatus?.status &&
            a.containerStatus?.exitCode === b.containerStatus?.exitCode
          )
        } else {
          return a === b
        }
      },
    },
  },
)

export const selectRecoveryPhraseCreated = (state: RootState) =>
  state.wallet.recoveryPhraseCreated
