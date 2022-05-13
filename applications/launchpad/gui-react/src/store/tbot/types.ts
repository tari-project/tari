export enum TBotMessageType {
  MiningHelp = 'miningHelp',
  WalletHelp = 'walletHelp',
  BaseNodeHelp = 'baseNodeHelp',
}

export type TBotMessage = {
  id: number
  // type: TBotMessageType
  message: string
}

export interface TBotState {
  messageQueue: TBotMessageType[]
}
