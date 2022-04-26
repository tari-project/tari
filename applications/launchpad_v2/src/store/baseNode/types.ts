import type { Network } from '../../containers/BaseNodeContainer/types'

export type BaseNodeState = {
  network: Network
  running: boolean
  pending: boolean
}
