import groupBy from 'lodash.groupby'

import { db } from '../../db'

import { StatsRepository } from './types'

const repositoryFactory: () => StatsRepository = () => {
  return {
    add: async (network, service, secondTimestamp, stats) => {
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
    getGroupedByContainer: async network => {
      const results = await db.stats
        ?.where('network')
        .equals(network)
        .sortBy('timestamp')

      return groupBy(results, 'service')
    },
  }
}

export default repositoryFactory
