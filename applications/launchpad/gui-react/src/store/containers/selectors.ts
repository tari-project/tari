import { createSelector } from '@reduxjs/toolkit'
import { RootState } from '../'

import { ContainerStatusDto, Container, SystemEventAction } from './types'

const noContainerData = (type: Container, error?: string) => ({
  id: undefined,
  type,
  status: undefined,
  pending: false,
  running: false,
  error: error,
})

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

const selectContainerError = (c: Container) => (r: RootState) => {
  if (r.containers.errors && r.containers.errors[c]) {
    return r.containers.errors[c]
  }

  return
}

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
        stats: {
          cpu: 0,
          memory: 0,
          unsubscribe: () => undefined,
        },
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

export const selectContainerWithMemo = (c: Container) =>
  createSelector(
    (s: RootState) => selectContainerByType(c)(s),
    selectPendingContainers,
    (s: RootState) => selectContainerError(c)(s),
    (container, pendingState, errorState) => {
      if (!container.containerId || !container.containerStatus) {
        return noContainerData(c, errorState)
      }

      const pending =
        pendingState.includes(c) || pendingState.includes(container.containerId)

      return {
        id: container.containerId,
        type: c,
        status: container.containerStatus.status,
        pending:
          pending ||
          (container.containerStatus.status !== SystemEventAction.Start &&
            container.containerStatus.status !== SystemEventAction.Destroy),
        running: container.containerStatus.status === SystemEventAction.Start,
        error: container.containerStatus.error || errorState,
      }
    },
  )
