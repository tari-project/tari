import { RootState } from '../'
import { Container } from '../containers/types'
import {
  selectRecipeRunning,
  selectRecipePending,
} from '../containers/selectors'
import type { Network } from '../../containers/BaseNodeContainer/types'

import { BaseNodeState } from './types'

export const selectState = (state: RootState): BaseNodeState => ({
  network: state.baseNode.network as Network,
  rootFolder: state.baseNode.rootFolder,
})

export const selectNetwork = (state: RootState) => state.baseNode.network
export const selectRootFolder = (state: RootState) => state.baseNode.rootFolder

export const selectRunning = selectRecipeRunning(Container.BaseNode)

export const selectPending = selectRecipePending(Container.BaseNode)

export const selectBaseNodeIdentity = (state: RootState) =>
  state.baseNode.identity
