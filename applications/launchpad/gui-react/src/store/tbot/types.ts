export enum TBotMessages {
  CryptoMiningHelp = 'CryptoMiningHelp',
  MergedMiningHelp = 'MergedMiningHelp',
  WalletHelp = 'WalletHelp',
  WalletIdHelp = 'WalletIdHelp',
  WalletBalanceHelp = 'WalletBalanceHelp',
  BaseNodeHelp = 'BaseNodeHelp',
  Onboarding = 'Onboarding',
  ConnectAurora = 'ConnectAurora',
  TransactionFee = 'TransactionFee',
  NewDockerImageToDownload = 'NewDockerImageToDownload',
  DockerImageDownloadSuccess = 'DockerImageDownloadSuccess',
  DockerImageDownloadError = 'DockerImageDownloadError',
}

export interface TBotState {
  messageQueue: string[]
}
