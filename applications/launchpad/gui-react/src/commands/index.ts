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
