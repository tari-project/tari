export enum TBotMessages {
  CryptoMiningHelp = 'CryptoMiningHelp',
  MergedMiningHelp = 'MergedMiningHelp',
  WalletHelp = 'WalletHelp',
  BaseNodeHelp = 'BaseNodeHelp',
}

export type TBotMessage = {
  id: number
  message: string
}

export interface TBotState {
  messageQueue: string[]
}
