import { RootState } from '../'
import { Container } from '../containers/types'
import { selectContainerStatus } from '../containers/selectors'
import type { Network } from '../../containers/BaseNodeContainer/types'

import { BaseNodeState } from './types'

export const selectState = (state: RootState): BaseNodeState => ({
  network: state.baseNode.network as Network,
})

const requiredContainers = [Container.Tor, Container.BaseNode]
export const selectContainerStatuses = (rootState: RootState) =>
  requiredContainers.map(containerType =>
    selectContainerStatus(containerType)(rootState),
  )

export const selectHealthy = (rootState: RootState) => {
  const containers = selectContainerStatuses(rootState)

  const unhealthy =
    containers.some(container => !container.pending && container.running) &&
    containers.some(container => !container.pending && !container.running)

  return !unhealthy
}
export const selectUnhealthyContainers = (rootState: RootState) => {
  const containers = selectContainerStatuses(rootState)
  return containers.filter(container => !container.running)
}
export const selectRunning = (rootState: RootState) => {
  const containers = selectContainerStatuses(rootState)

  return Boolean(containers.filter(container => container.running).length)
}
export const selectPending = (rootState: RootState) => {
  const containers = selectContainerStatuses(rootState)

  return containers.some(container => container.pending)
}
