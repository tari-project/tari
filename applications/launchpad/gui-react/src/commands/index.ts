import { invoke, os } from '@tauri-apps/api'
import { ChildProcess, Command } from '@tauri-apps/api/shell'

/**
 * Check the Docker version on the host machine
 * @returns {Promise<ChildProcess>}
 */
export const dockerVersionCmd = async (): Promise<ChildProcess> => {
  const command = new Command('docker', ['-v'])
  return command.execute()
}

/**
 * Check if Docker is installed on the host machine
 * @returns {Promise<boolean>}
 */
export const isDockerInstalled = async (): Promise<boolean> => {
  const dockerVerCmd = await dockerVersionCmd()
  return Boolean(dockerVerCmd.stdout.match(/docker version/i))
}

/**
 * Open the Terminal
 */
export const openTerminalCmd = async () => {
  try {
    const detectedPlatform = await os.type()

    if (
      !['linux', 'windows_nt', 'darwin'].includes(
        detectedPlatform.toLowerCase(),
      )
    ) {
      return
    }

    const platform = detectedPlatform.toLowerCase() as
      | 'linux'
      | 'windows_nt'
      | 'darwin'

    invoke('open_terminal', {
      platform: platform,
    })
  } catch (err) {
    // eslint-disable-next-line no-console
    console.error(err)
    return
  }
}
