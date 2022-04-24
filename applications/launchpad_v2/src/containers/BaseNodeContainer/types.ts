export type Network = 'mainnet' | 'testnet'

export interface BaseNodeProps {
  startNode: () => void
  stopNode: () => void
  tariNetwork: Network
  setTariNetwork: (selected: Network) => void
  running: boolean
  pending: boolean
}

export type BaseNodeState = {
  network: Network
  running: boolean
  pending: boolean
}
