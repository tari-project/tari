import type { UnlistenFn } from '@tauri-apps/api/event'

import { ContainerName } from '../../types/general'

export type ContainerId = string

// WARNING - be careful about using this directly,
// you should be using dockerImages.images from state if you work with docker images etc
// this *couples fronted to backend* with container_name in backend/src/docker/models.rs
export enum Container {
  Tor = 'tor',
  BaseNode = 'base_node',
  Wallet = 'wallet',
  SHA3Miner = 'sha3_miner',
  MMProxy = 'mm_proxy',
  XMrig = 'xmrig',
  Monerod = 'monerod',
  Loki = 'loki',
  Promtail = 'promtail',
  Grafana = 'grafana',
}

export enum SystemEventAction {
  Destroy = 'destroy',
  Create = 'create',
  Start = 'start',
  Die = 'die',
}

export type ServiceDescriptor = {
  id: ContainerId
  logEventsName: string
  statsEventsName: string
  name: string
}

export type ContainerStats = {
  cpu: number
  memory: number
  network: {
    upload: number
    download: number
  }
  unsubscribe: UnlistenFn
}
export type SerializableContainerStats = Omit<ContainerStats, 'unsubscribe'>

export type ContainerStatus = {
  status: SystemEventAction
  timestamp: number
  name?: string
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  error?: any
  exitCode?: number
  eventsChannel?: string
}

export type ContainerStatusDto = {
  id: ContainerId
  containerName: ContainerName
  running: boolean
  pending: boolean
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  error?: any
}

export type ContainerStatusDtoWithStats = ContainerStatusDto & {
  stats: ContainerStats
}

export type ContainerStateFields = Pick<
  ContainerStatusDto,
  'running' | 'pending' | 'error'
>

export type ContainerStateFieldsWithIdAndType = ContainerStateFields &
  Pick<ContainerStatusDto, 'id' | 'containerName'>

export type ContainersState = {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  errors: Record<ContainerName, any>
  pending: Array<ContainerName | ContainerId>
  containers: Record<ContainerId, ContainerStatus>
  stats: Record<ContainerId, ContainerStats>
}

export interface StatsEventPayload {
  read: string
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
  networks: Record<string, { tx_bytes: number; rx_bytes: number }>
}
