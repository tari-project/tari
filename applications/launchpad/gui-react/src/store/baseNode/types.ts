import type { Network } from '../../containers/BaseNodeContainer/types'

export interface BaseNodeIdentityDto {
  publicKey: number[]
  nodeId: number[]
  publicAddress: string
  emojiId: string
}

export interface BaseNodeIdentity {
  publicKey: string
  nodeId: string
  publicAddress: string
  emojiId: string
}

export type BaseNodeState = {
  network: Network
  rootFolder: string
  identity?: BaseNodeIdentity
}
