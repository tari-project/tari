export enum WalletSetupRequired {
  MissingWalletAddress = 'missing_wallet_address',
}

export type WalletState = {
  unlocked: boolean
  address: string
  tari: {
    balance: number
    available: number
    pending: boolean
  }
}
