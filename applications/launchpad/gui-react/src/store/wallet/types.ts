export enum WalletSetupRequired {
  MissingWalletAddress = 'missing_wallet_address',
}

export type WalletState = {
  running: boolean
  unlocked: boolean
  pending: boolean
  address: string
  tari: {
    balance: number
    available: number
    pending: boolean
  }
}
