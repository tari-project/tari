export enum Settings {
  Mining = 'mining',
  Wallet = 'wallet',
  BaseNode = 'baseNode',
  Docker = 'docker',
  Logs = 'logs',
  Security = 'security',
}

export interface InitialSettings {
  walletPassword: string
  moneroMiningAddress: string
  moneroWalletAddress: string
  numMiningThreads: number
  tariNetwork: string
  cacheDir: string
  dockerRegistry: string
  dockerTag: string
  monerodUrl: string
  moneroUseAuth: boolean
  moneroUsername: string
  moneroPassword: string
  rootFolder: string
}

export type SettingsState = {
  open: boolean
  which: Settings
  serviceSettings: {
    walletPassword: string
    dockerRegistry: string
    dockerTag: string
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
  } & any
}
