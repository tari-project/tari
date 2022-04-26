export type Network = 'mainnet' | 'testnet'

export type BaseNodeState = {
  network: Network
  running: boolean
  pending: boolean
}
