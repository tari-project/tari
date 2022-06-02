import type { UnlistenFn } from '@tauri-apps/api/event'

export type ContainerId = string

export enum Container {
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
  unsubscribe: UnlistenFn
}
export type SerializableContainerStats = Omit<ContainerStats, 'unsubscribe'>

export type ContainerStatus = {
  status: SystemEventAction
  timestamp: number
  type?: Container
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  error?: any
}

export type ContainerStatusDto = {
  id: ContainerId
  type: Container
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
  Pick<ContainerStatusDto, 'id' | 'type'>

export type ContainersState = {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  errors: Record<Container, any>
  pending: Array<Container | ContainerId>
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

export type StatsDbEntry = SerializableContainerStats & { timestamp: string }
export interface StatsRepository {
  add: (
    network: string,
    service: Container,
    secondTimestamp: string,
    stats: SerializableContainerStats,
  ) => Promise<void>
  getAll: (network: string, service: Container) => Promise<StatsDbEntry[]>
  getGroupedByContainer: (
    network: string,
  ) => Promise<Record<Container, StatsDbEntry[]>>
}
