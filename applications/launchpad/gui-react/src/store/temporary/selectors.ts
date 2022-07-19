import { RootState } from '..'

export const selectWalletPasswordConfirmation = (state: RootState) =>
  state.temporary.walletPasswordConfirmation
