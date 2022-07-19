import { invoke, os } from '@tauri-apps/api'

export type DockerVersion = {
  ApiVersion: string
  Arch: string
  BuildTime: string
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  Components: any[]
  GitCommit: string
  GoVersion: string
  KernelVersion: string
  MinAPIVersion: string
  Os: string
  Platform: { Name: string }
  Version: string
  experimental: null
}

/**
 * Check the Docker version on the host machine
 * @returns {Promise<VersionInfo>}
 */
export const checkDocker = async () => {
  try {
    const res: DockerVersion = await invoke('check_docker')
    return res
  } catch (err) {
    // eslint-disable-next-line no-console
    console.error('Error: invoke(check_error)', err)
    return
  }
}

/**
 * Check if Docker is installed on the host machine
 * @returns {Promise<boolean>}
 */
export const isDockerInstalled = async (): Promise<boolean> => {
  const dockerVerCmd = await checkDocker()
  return Boolean(dockerVerCmd)
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
