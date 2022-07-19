export enum WalletSetupRequired {
  MissingWalletAddress = 'missing_wallet_address',
}

export type WalletState = {
  unlocked: boolean
  address: {
    uri: string
    emoji: string
    publicKey: string
  }
  tari: {
    balance: number
    available: number
    pending: boolean
  }
  lastTxHistoryUpdateAt?: Date
  recoveryPhraseCreated: boolean
}
