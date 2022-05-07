import type { UnlistenFn } from '@tauri-apps/api/event'

export enum Service {
  Tor = 'tor',
  BaseNode = 'base_node',
  Wallet = 'wallet',
  SHA3Miner = 'sha3_miner',
  MMProxy = 'mm_proxy',
  XMrig = 'xmrig',
  Monerod = 'monerod',
  Frontail = 'frontail',
}

export type ServiceId = string

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
  stats: {
    cpu: number
    memory: number
    unsubscribe: UnlistenFn
  }
}

export type ServicesState = {
  services: Record<string, unknown>
  servicesStatus: Record<Service, ServiceStatus>
}
