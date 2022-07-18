import { createSelector } from '@reduxjs/toolkit'

import { RootState } from '../'
import { ContainerName } from '../../types/general'
import { selectDockerImages, selectRecipe } from '../dockerImages/selectors'

import {
  ContainerStatusDto,
  Container,
  SystemEventAction,
  ContainerStatusDtoWithStats,
} from './types'

export const selectState = (rootState: RootState) => rootState.containers

export const selectContainer = (c: ContainerName) => (r: RootState) => {
  const containers = Object.entries(r.containers.containers).filter(
    ([, value]) => value.name === c,
  )
  containers.sort(([, a], [, b]) => b.timestamp - a.timestamp)
  const [containerId, containerStatus] = containers[0] || []

  return { containerId, containerStatus }
}

export const selectContainerError = (c: ContainerName) => (r: RootState) => {
  return r.containers.errors[c]
}

const selectContainerStats = (containerId: string) => (r: RootState) =>
  r.containers.stats[containerId]

type ContainerStatusSelector = (
  c: ContainerName,
) => (r: RootState) => ContainerStatusDto
export const selectContainerStatus: ContainerStatusSelector =
  containerName => rootState => {
    const { containerId, containerStatus } =
      selectContainer(containerName)(rootState)

    const pending =
      rootState.containers.pending.includes(containerName) ||
      rootState.containers.pending.includes(containerId)

    const typeError = rootState.containers.errors[containerName]

    if (!containerId) {
      return {
        id: '',
        containerName,
        error: typeError,
        running: false,
        pending,
      }
    }

    const { name: _, ...containerStatusWithoutName } = containerStatus
    return {
      ...containerStatusWithoutName,
      id: containerId,
      pending:
        pending ||
        (containerStatus.status !== SystemEventAction.Start &&
          containerStatus.status !== SystemEventAction.Destroy &&
          containerStatus.status !== SystemEventAction.Die),
      running: containerStatus.status === SystemEventAction.Start,
      containerName,
      error: containerStatus.error || typeError,
    }
  }

type ContainerStatusSelectorWithStats = (
  c: ContainerName,
) => (r: RootState) => ContainerStatusDtoWithStats
export const selectContainerStatusWithStats: ContainerStatusSelectorWithStats =
  containerName => rootState => {
    const container = selectContainerStatus(containerName)(rootState)

    if (!container.id) {
      return {
        ...container,
        stats: {
          cpu: 0,
          memory: 0,
          network: {
            download: 0,
            upload: 0,
          },
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
      selectContainerStatus(containerStatus.name as Container)(rootState),
    )
    .filter(status => status.running)
    .map(status => rootState.containers.containers[status.id].name as Container)

export const selectContainersStatusesWithStats = createSelector(
  selectDockerImages,
  (rootState: RootState) => rootState,
  (dockerImages, rootState) =>
    dockerImages.map(dockerImage => ({
      ...dockerImage,
      container: dockerImage.containerName,
      status: selectContainerStatusWithStats(dockerImage.containerName)(
        rootState,
      ),
    })),
)

const selectContainerStatusesByRecipe =
  (containerName: ContainerName) => (rootState: RootState) => {
    const recipe = selectRecipe(containerName)(rootState)
    return recipe.map(containerType =>
      selectContainerStatus(containerType)(rootState),
    )
  }

export const selectRecipeRunning = (containerName: ContainerName) =>
  createSelector(
    selectContainerStatusesByRecipe(containerName),
    containers =>
      containers.every(container => container.running) ||
      containers.some(container => container.running && container.pending),
  )

export const selectRecipePending = (containerName: ContainerName) =>
  createSelector(selectContainerStatusesByRecipe(containerName), containers =>
    containers.some(container => container.pending),
  )

export const selectAllContainerEventsChannels = (rootState: RootState) =>
  Object.values(rootState.containers.containers)
    .map(container => ({
      status: selectContainerStatus(container.name as Container)(rootState),
      container,
    }))
    .filter(
      ({ container, status }) => container.eventsChannel && status.running,
    )
    .map(({ container }) => ({
      service: container.name,
      eventsChannel: container.eventsChannel,
    }))
