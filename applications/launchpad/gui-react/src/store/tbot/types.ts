export enum TBotMessages {
  CryptoMiningHelp = 'CryptoMiningHelp',
  MergedMiningHelp = 'MergedMiningHelp',
  WalletHelp = 'WalletHelp',
  WalletBalanceHelp = 'WalletBalanceHelp',
  BaseNodeHelp = 'BaseNodeHelp',
  Onboarding = 'Onboarding',
  ConnectAurora = 'ConnectAurora',
}

export interface TBotState {
  messageQueue: string[]
}
