import groupBy from 'lodash.groupby'

import { Dictionary } from '../../types/general'
import { db } from '../../db'

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

const repositoryFactory: () => StatsRepository = () => {
  return {
    add: async (network, service, secondTimestamp, stats) => {
      // overwrites stats entry for specific timestamp and network key
      db.stats.put(
        {
          timestamp: secondTimestamp,
          network,
          service,
          cpu: stats.cpu,
          memory: stats.memory,
          upload: stats.network.upload,
          download: stats.network.download,
        },
        [secondTimestamp, network],
      )
    },
    getGroupedByContainer: async (network, from, to) => {
      const results = await db.stats
        ?.where('network')
        .equals(network)
        .and(item => item.timestamp >= from.toISOString())
        .and(item => item.timestamp <= to.toISOString())
        .sortBy('timestamp')

      return groupBy(results, 'service')
    },
  }
}

export default repositoryFactory
