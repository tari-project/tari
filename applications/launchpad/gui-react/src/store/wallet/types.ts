export type WalletState = {
  running: boolean
  unlocked: boolean
  pending: boolean
  address: string
  tari: {
    balance: number
    available: number
  }
}
