import { createSelector } from '@reduxjs/toolkit'

import { RootState } from '../'
import { Container } from '../containers/types'
import { selectContainerStatus } from '../containers/selectors'

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

const requiredContainers = [Container.Tor, Container.Wallet]
export const selectContainerStatuses = (rootState: RootState) =>
  requiredContainers.map(containerType =>
    selectContainerStatus(containerType)(rootState),
  )
export const selectIsPending = createSelector(
  selectContainerStatuses,
  containers => containers.some(container => container.pending),
)
export const selectIsRunning = createSelector(
  selectContainerStatuses,
  containers =>
    containers.every(container => container.running) ||
    containers.some(container => container.running && container.pending),
)
