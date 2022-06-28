import { createSelector } from '@reduxjs/toolkit'

import { RootState } from '../'
import { Container } from '../containers/types'
import { selectContainerStatus } from '../containers/selectors'
import { selectRecipe } from '../dockerImages/selectors'
import type { Network } from '../../containers/BaseNodeContainer/types'

import { BaseNodeState } from './types'

export const selectState = (state: RootState): BaseNodeState => ({
  network: state.baseNode.network as Network,
})

export const selectNetwork = (state: RootState) => state.baseNode.network

const selectContainerStatuses = (rootState: RootState) => {
  const recipe = selectRecipe(Container.BaseNode)(rootState)
  return recipe.map(containerType =>
    selectContainerStatus(containerType)(rootState),
  )
}

export const selectRunning = createSelector(
  selectContainerStatuses,
  containers =>
    containers.every(container => container.running) ||
    containers.some(container => container.running && container.pending),
)

export const selectPending = createSelector(
  selectContainerStatuses,
  containers => containers.some(container => container.pending),
)
