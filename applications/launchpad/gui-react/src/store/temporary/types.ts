export type WalletPasswordConfirmationStatus =
  | 'waiting_for_confirmation'
  | 'failed'
  | 'wrong_password'
  | 'success'

export interface TemporaryState {
  walletPasswordConfirmation?: WalletPasswordConfirmationStatus
}
