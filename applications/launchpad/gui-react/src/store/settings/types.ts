export enum Settings {
  Mining = 'mining',
  Wallet = 'wallet',
  BaseNode = 'baseNode',
  Docker = 'docker',
  Logs = 'logs',
  Security = 'security',
}

export type SettingsState = {
  open: boolean
  which: Settings
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  serviceSettings: any
}
