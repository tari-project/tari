import { RootState } from '../'

import { Service, SystemEventAction } from './types'

export const selectState = (rootState: RootState) => rootState.services

export const selectServiceStatus =
  (service: Service) => (rootState: RootState) => {
    const serviceState = rootState.services.services[service]

    if (!serviceState.containerId) {
      return {
        running: false,
        pending: serviceState.pending,
        id: '',
        stats: {
          cpu: 0,
          memory: 0,
          unsubscribe: () => null,
        },
      }
    }

    const containerStatus =
      rootState.services.containers[serviceState.containerId]

    return {
      ...containerStatus,
      pending:
        serviceState.pending ||
        (containerStatus.lastAction !== SystemEventAction.Start &&
          containerStatus.lastAction !== SystemEventAction.Destroy),
      running: true,
    }
  }

export const selectRunningServices = (rootState: RootState): Service[] =>
  Object.entries(rootState.services.services)
    .map(([service]) => ({
      service,
      status: selectServiceStatus(service as Service)(rootState),
    }))
    .filter(({ status }) => status.running)
    .map(({ service }) => service as Service)

export const selectAllServicesStatuses = (rootState: RootState) =>
  Object.entries(rootState.services.services).map(([service]) => ({
    service,
    status: selectServiceStatus(service as Service)(rootState),
  }))
