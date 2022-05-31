export enum TBotMessages {
  CryptoMiningHelp = 'CryptoMiningHelp',
  MergedMiningHelp = 'MergedMiningHelp',
  WalletHelp = 'WalletHelp',
  BaseNodeHelp = 'BaseNodeHelp',
}

export type TBotMessage = {
  id: number
  // type: TBotMessageType
  message: string
}

export interface TBotState {
  messageQueue: string[]
}
