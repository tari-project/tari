export enum TBotMessages {
  CryptoMiningHelp = 'cryptoMiningHelp',
  MergedMiningHelp = 'mergedMiningHelp',
  WalletHelp = 'walletHelp',
  BaseNodeHelp = 'baseNodeHelp',
}

export type TBotMessage = {
  id: number
  // type: TBotMessageType
  message: string
}

export interface TBotState {
  messageQueue: string[]
}
