import { RootState } from '../'

import { Service, ServiceStatus } from './types'

export const selectState = (rootState: RootState) => rootState.services

export const selectServiceStatus =
  (service: Service) =>
  (rootState: RootState): ServiceStatus =>
    rootState.services.servicesStatus[service]

export const selectRunningServices = (rootState: RootState): Service[] =>
  Object.entries(rootState.services.servicesStatus)
    .filter(([, status]) => status.running)
    .map(([service]) => service as Service)
