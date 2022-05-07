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

export enum SystemEventAction {
  Destroy = 'destroy',
  Create = 'create',
  Start = 'start',
}

export type ServiceDescriptor = {
  id: ContainerId
  logEventsName: string
  statsEventsName: string
  name: string
}

export type ServiceStatus = {
  id: ContainerId
  lastAction: SystemEventAction
  error?: string
  stats: {
    cpu: number
    memory: number
    unsubscribe: UnlistenFn
  }
}

export type ContainerId = string

export type ServicesState = {
  containers: Record<ContainerId, ServiceStatus>
  services: Record<
    Service,
    {
      pending: boolean
      containerId: ContainerId
    }
  >
}

export interface StatsEventPayload {
  precpu_stats: {
    cpu_usage: {
      total_usage: number
    }
    system_cpu_usage: number
  }
  cpu_stats: {
    cpu_usage: {
      total_usage: number
    }
    system_cpu_usage: number
    online_cpus: number
  }
  memory_stats: {
    usage: number
    stats: {
      cache?: number
    }
  }
}
