import { mockIPC } from '@tauri-apps/api/mocks'

/**
 * Set of default values returned by `tauriIPCMock()`
 */
export const defaultTauriMockValues: Record<string, unknown> = {
  os: {
    arch: 'x86_64',
    platform: 'darwin',
    ostype: 'Darwin',
  },
}

/**
 * The Tauri IPC mock.
 *
 * It uses Tauri's mockIPC and returns the value set in the `props`.
 * If nothing found in `props`, it will return a value from `defaultTauriMockValues`.
 *
 * @param {Record<string, unknown>} props - pass the value you expect in tests
 *
 * @example
 * // Use default values:
 * tauriIPCMock()
 *
 * // Get given value from specific API module (ie. 'platform' from 'os' module)
 * tauriIPCMock({
 *   os: {
 *     platform: 'darwin',
 *   },
 * })
 */
export const tauriIPCMock = (props: Record<string, unknown> = undefined) => {
  return mockIPC((cmd, args) => {
    switch (cmd) {
      case 'tauri':
        return tauriCmdMock(cmd, args, props)
      case 'invoke':
        return
      default:
        return
    }
  })
}

const tauriCmdMock = (
  cmd: string,
  args: Record<string, unknown>,
  props: Record<string, unknown>,
) => {
  const tauriModule = (args?.__tauriModule as string)?.toLowerCase()
  const messageCmd = (args?.message as { cmd?: string })?.cmd?.toLowerCase()

  if (tauriModule && messageCmd) {
    if (
      props &&
      Object.keys(props).includes(tauriModule) &&
      Object.keys(props[tauriModule]).includes(messageCmd)
    ) {
      return props[tauriModule][messageCmd]
    } else if (
      Object.keys(defaultTauriMockValues).includes(tauriModule) &&
      Object.keys(defaultTauriMockValues[tauriModule]).includes(messageCmd)
    ) {
      return defaultTauriMockValues[tauriModule][messageCmd]
    }
  }

  return
}
