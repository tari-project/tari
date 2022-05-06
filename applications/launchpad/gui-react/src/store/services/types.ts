export enum Service {
  Tor = 'tor',
  BaseNode = 'base_node',
  Wallet = 'wallet',
  SHA3Miner = 'sha3_miner',
  MMProxy = 'mm_proxy',
}

type ServiceId = string

export type ServiceDescriptor = {
  id: ServiceId
  logEventsName: string
  statsEventsName: string
  name: string
}

export type ServiceStatus = {
  id: ServiceId
  pending: boolean
  running: boolean
  error?: string
}

export type ServicesState = {
  services: Record<string, unknown>
  servicesStatus: Record<Service, ServiceStatus>
}
