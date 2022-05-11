import { RootState } from '../'

import { ContainerStatusDto, Container, SystemEventAction } from './types'

export const selectState = (rootState: RootState) => rootState.containers

const selectContainerByType = (c: Container) => (r: RootState) => {
  const containers = Object.entries(r.containers.containers).filter(
    ([, value]) => value.type === c,
  )
  containers.sort(([, a], [, b]) => b.timestamp - a.timestamp)
  const [containerId, containerStatus] = containers[0] || []

  return { containerId, containerStatus }
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

    if (!containerId) {
      return {
        id: '',
        type: containerType,
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
