import { RootState } from '../'

import {
  ContainerStatusDto,
  Container,
  SystemEventAction,
  ContainerStatusDtoWithStats,
} from './types'

export const selectState = (rootState: RootState) => rootState.containers

export const selectPendingContainers = (rootState: RootState) =>
  rootState.containers.pending

export const selectContainerByType = (c: Container) => (r: RootState) => {
  const containers = Object.entries(r.containers.containers).filter(
    ([, value]) => value.type === c,
  )
  containers.sort(([, a], [, b]) => b.timestamp - a.timestamp)
  const [containerId, containerStatus] = containers[0] || []

  return { containerId, containerStatus }
}

export const selectContainerStats = (containerId: string) => (r: RootState) =>
  r.containers.stats[containerId]

type ContainerStatusSelector = (
  c: Container,
) => (r: RootState) => ContainerStatusDto
export const selectContainerStatus: ContainerStatusSelector =
  containerType => rootState => {
    const { containerId, containerStatus } =
      selectContainerByType(containerType)(rootState)

    const pending =
      rootState.containers.pending.includes(containerType) ||
      rootState.containers.pending.includes(containerId)

    const typeError = rootState.containers.errors[containerType]

    if (!containerId) {
      return {
        id: '',
        type: containerType,
        error: typeError,
        running: false,
        pending,
      }
    }

    return {
      ...containerStatus,
      id: containerId,
      pending:
        pending ||
        (containerStatus.status !== SystemEventAction.Start &&
          containerStatus.status !== SystemEventAction.Destroy),
      running: containerStatus.status === SystemEventAction.Start,
      type: containerType,
      error: containerStatus.error || typeError,
    }
  }

type ContainerStatusSelectorWithStats = (
  c: Container,
) => (r: RootState) => ContainerStatusDtoWithStats
export const selectContainerStatusWithStats: ContainerStatusSelectorWithStats =
  containerType => rootState => {
    const container = selectContainerStatus(containerType)(rootState)

    if (!container.id) {
      return {
        ...container,
        stats: {
          cpu: 0,
          memory: 0,
          timestamp: '',
          unsubscribe: () => undefined,
        },
      }
    }

    const containerStats = selectContainerStats(container.id)(rootState)

    return {
      ...container,
      stats: containerStats,
    }
  }

export const selectRunningContainers = (rootState: RootState): Container[] =>
  Object.entries(rootState.containers.containers)
    .map(([, containerStatus]) =>
      selectContainerStatus(containerStatus.type as Container)(rootState),
    )
    .filter(status => status.running)
    .map(status => rootState.containers.containers[status.id].type as Container)

export const selectContainersStatuses = (rootState: RootState) =>
  Object.values(Container).map(type => ({
    container: type,
    status: selectContainerStatus(type as Container)(rootState),
  }))

export const selectContainersStatusesWithStats = (rootState: RootState) =>
  Object.values(Container).map(type => ({
    container: type,
    status: selectContainerStatusWithStats(type as Container)(rootState),
  }))
