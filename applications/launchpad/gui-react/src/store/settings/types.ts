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
  serviceSettings: unknown
}
