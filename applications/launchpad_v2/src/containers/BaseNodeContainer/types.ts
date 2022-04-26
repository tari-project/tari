import type { Network } from '../../store/baseNode/types'

export interface BaseNodeProps {
  startNode: () => void
  stopNode: () => void
  tariNetwork: Network
  setTariNetwork: (selected: Network) => void
  running: boolean
  pending: boolean
}
