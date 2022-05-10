import { ContainerStatusDto } from '../../store/containers/types'

export type Network = 'dibbler' | 'testnet'

export interface BaseNodeProps {
  startNode: () => void
  stopNode: () => void
  tariNetwork: Network
  setTariNetwork: (selected: Network) => void
  openExpertView: () => void
  running: boolean
  pending: boolean
  healthy: boolean
  unhealthyContainers: ContainerStatusDto[]
}
