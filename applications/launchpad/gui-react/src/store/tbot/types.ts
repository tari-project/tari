export enum TBotMessages {
  CryptoMiningHelp = 'CryptoMiningHelp',
  MergedMiningHelp = 'MergedMiningHelp',
  WalletHelp = 'WalletHelp',
  WalletIdHelp = 'WalletIdHelp',
  WalletBalanceHelp = 'WalletBalanceHelp',
  BaseNodeHelp = 'BaseNodeHelp',
  Onboarding = 'Onboarding',
  ConnectAurora = 'ConnectAurora',
}

export interface TBotState {
  messageQueue: string[]
}
