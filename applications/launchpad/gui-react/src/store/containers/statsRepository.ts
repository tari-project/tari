import Database from 'tauri-plugin-sql-api'
import groupby from 'lodash.groupby'

import { Dictionary } from '../../types/general'

import { Container, SerializableContainerStats } from './types'

let db: Database
const getDb = async () => {
  if (!db) {
    db = await Database.load('sqlite:launchpad.db')
  }

  return db
}
// load immediately to avoid waiting with first query
getDb()

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
      const db = await getDb()

      await db.execute(
        `INSERT INTO stats(timestamp, network, service, cpu, memory, upload, download) VALUES($1, $2, $3, $4, $5, $6, $7)
           ON CONFLICT(timestamp, network, service)
           DO UPDATE SET
            "insertsPerTimestamp"="insertsPerTimestamp"+1,
            cpu=(cpu+$4)/("insertsPerTimestamp"+1),
            memory=(memory+$5)/("insertsPerTimestamp"+1),
            upload=(upload+$6)/("insertsPerTimestamp"+1),
            download=(download+$7)/("insertsPerTimestamp"+1)`,
        [
          secondTimestamp,
          network,
          service,
          stats.cpu,
          stats.memory,
          stats.network.upload,
          stats.network.download,
        ],
      )
    },
    getGroupedByContainer: async (network, from, to) => {
      const db = await getDb()

      const results: StatsEntry[] = await db.select(
        'SELECT * FROM stats WHERE network = $1 AND "timestamp" >= $2 AND "timestamp" <= $3 ORDER BY "timestamp"',
        [network, from, to],
      )

      return groupby(results, 'service')
    },
  }
}

export default repositoryFactory
