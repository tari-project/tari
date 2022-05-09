import { RootState } from '../'

import { ServiceStatus, Service, SystemEventAction } from './types'

export const selectState = (rootState: RootState) => rootState.services

type ServiceStatusSelector = (s: Service) => (r: RootState) => ServiceStatus
export const selectServiceStatus: ServiceStatusSelector =
  service => rootState => {
    const serviceState = rootState.services.services[service]

    if (!serviceState.containerId) {
      return {
        running: false,
        pending: serviceState.pending,
        stats: {
          cpu: 0,
          memory: 0,
          unsubscribe: () => undefined,
        },
      }
    }

    const { lastAction, ...containerStatus } =
      rootState.services.containers[serviceState.containerId]

    return {
      ...containerStatus,
      pending:
        serviceState.pending ||
        (lastAction !== SystemEventAction.Start &&
          lastAction !== SystemEventAction.Destroy),
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
