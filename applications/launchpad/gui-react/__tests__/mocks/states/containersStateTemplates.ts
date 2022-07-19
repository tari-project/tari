import {
  Container,
  ContainerId,
  ContainerStats,
  ContainerStatus,
  ContainersState,
  SystemEventAction,
} from '../../../src/store/containers/types'

const noErrors = {
  [Container.Tor]: undefined,
  [Container.BaseNode]: undefined,
  [Container.Wallet]: undefined,
  [Container.SHA3Miner]: undefined,
  [Container.MMProxy]: undefined,
  [Container.XMrig]: undefined,
  [Container.Monerod]: undefined,
  [Container.Loki]: undefined,
  [Container.Promtail]: undefined,
  [Container.Grafana]: undefined,
}

const runningContainers = (cs: Container[]) => {
  const containers: Record<ContainerId, ContainerStatus> = {}
  const stats: Record<ContainerId, ContainerStats> = {}

  cs.forEach(c => {
    containers[`${c.toLowerCase()}-id`] = {
      name: c,
      error: undefined,
      status: SystemEventAction.Start,
      timestamp: Number(Date.now()),
    }

    stats[`${c.toLowerCase()}-id`] = {
      network: { upload: 0, download: 0 },
      cpu: 0,
      memory: 0,
      unsubscribe: () => {
        return
      },
    }
  })

  return containers
}

const zeroedStatsForContainers = (cs: Container[]) => {
  const stats: Record<ContainerId, ContainerStats> = {}

  cs.forEach(c => {
    stats[`${c.toLowerCase()}-id`] = {
      network: { upload: 0, download: 0 },
      cpu: 0,
      memory: 0,
      unsubscribe: () => {
        return
      },
    }
  })

  return stats
}

export const allStopped: ContainersState = {
  errors: noErrors,
  pending: [],
  containers: {},
  stats: {},
}

export const tariContainersRunning: ContainersState = {
  errors: noErrors,
  pending: [],
  containers: {
    ...runningContainers([
      Container.Tor,
      Container.BaseNode,
      Container.Wallet,
      Container.SHA3Miner,
    ]),
  },
  stats: {
    ...zeroedStatsForContainers([
      Container.Tor,
      Container.BaseNode,
      Container.Wallet,
      Container.SHA3Miner,
    ]),
  },
}
