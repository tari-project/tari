import { Dictionary } from '../../types/general'

import { Container, SerializableContainerStats } from './types'

export interface StatsEntry {
  timestamp: string
  network: string
  service: Container
  cpu: number
  memory: number
  upload: number
  download: number
}

export interface StatsRepository {
  add: (
    network: string,
    service: Container,
    secondTimestamp: string,
    stats: SerializableContainerStats,
  ) => Promise<void>
  getGroupedByContainer: (
    network: string,
    from: Date,
    to: Date,
  ) => Promise<Dictionary<StatsEntry[]>>
}

const storage = new Map<Container, StatsEntry[]>()

// TODO implement sqlite
const repositoryFactory: () => StatsRepository = () => {
  return {
    add: async (network, service, secondTimestamp, stats) => {
      if (!storage.has(service)) {
        storage.set(service, [])
      }

      // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
      storage!.get(service)?.push({
        timestamp: secondTimestamp,
        network,
        service,
        cpu: stats.cpu,
        memory: stats.memory,
        upload: stats.network.upload,
        download: stats.network.download,
      })
    },
    getGroupedByContainer: async (_network, from, to) => {
      return Object.values(Container).reduce((accu, current) => {
        if (storage.has(current)) {
          return {
            ...accu,
            [current]: storage
              .get(current)
              ?.filter(
                item =>
                  item.timestamp >= from.toISOString() &&
                  item.timestamp <= to.toISOString(),
              ),
          } as Dictionary<StatsEntry[]>
        }

        return accu
      }, {} as Dictionary<StatsEntry[]>)
    },
  }
}

export default repositoryFactory
