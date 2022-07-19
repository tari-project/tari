import { ContainerName } from '../types/general'
import { SerializableContainerStats } from '../store/containers/types'
import { db } from './db'

export interface StatsEntry {
  timestamp: string
  timestampS: number
  network: string
  service: ContainerName
  cpu: number | null
  memory: number | null
  upload: number | null
  download: number | null
}

export interface StatsRepository {
  add: (
    network: string,
    container: ContainerName,
    secondTimestamp: string,
    stats: SerializableContainerStats,
  ) => Promise<void>
  getEntries: (network: string, since: Date) => Promise<StatsEntry[]>
  removeOld: (age?: number) => Promise<void>
}

const repositoryFactory: () => StatsRepository = () => {
  return {
    add: async (network, container, secondTimestamp, stats) => {
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
          container,
          stats.cpu,
          stats.memory,
          stats.network.upload,
          stats.network.download,
        ],
      )
    },
    removeOld: async (age = 24 * 3600 * 1000) => {
      const nowTS = new Date().getTime()
      const whenTS = new Date(nowTS - age)
      await db.execute('DELETE from stats WHERE "timestamp" < $1', [
        whenTS.toISOString(),
      ])
    },
    getEntries: async (network, since) => {
      const results: Omit<StatsEntry, 'timestampS'>[] = await db.select(
        'SELECT timestamp, service, cpu, memory, upload, download FROM stats WHERE network = $1 AND "timestamp" > $2 ORDER BY "timestamp"',
        [network, since.toISOString()],
      )

      return results.map(r => ({
        ...r,
        timestampS: new Date(r.timestamp).getTime() / 1000,
      }))
    },
  }
}

export default repositoryFactory
