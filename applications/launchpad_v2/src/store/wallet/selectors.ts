import { RootState } from '../'

export const selectState = (state: RootState) => state.wallet
export const selectIsUnlocked = (state: RootState) => state.wallet.unlocked
export const selectWalletAddress = (state: RootState) => state.wallet.address
export const selectTariAmount = (state: RootState) => state.wallet.tari
export const selectIsPending = (state: RootState) => state.wallet.pending
