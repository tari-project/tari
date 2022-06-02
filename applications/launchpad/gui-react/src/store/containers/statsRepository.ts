import { StatsDbEntry, StatsRepository, Container } from './types'

const db: Record<string, Record<Container, StatsDbEntry[]>> = {}

const ensureDb = (network: string) => {
  if (!db[network]) {
    db[network] = {
      [Container.Tor]: [] as StatsDbEntry[],
      [Container.BaseNode]: [] as StatsDbEntry[],
      [Container.Wallet]: [] as StatsDbEntry[],
      [Container.SHA3Miner]: [] as StatsDbEntry[],
      [Container.MMProxy]: [] as StatsDbEntry[],
      [Container.XMrig]: [] as StatsDbEntry[],
      [Container.Monerod]: [] as StatsDbEntry[],
      [Container.Frontail]: [] as StatsDbEntry[],
    }
  }
}

// TODO implement actual db https://github.com/Altalogy/tari/issues/46
// THIS IS A TEMPORARY IMPLEMENTATION
// this is the dumbest implementation I could write
// future implementation will use Dexie.js
const MAX_ENTRIES = 1800
const repositoryFactory: () => StatsRepository = () => {
  return {
    add: async (network, service, secondTimestamp, stats) => {
      ensureDb(network)

      if (db[network][service].length >= MAX_ENTRIES) {
        db[network][service].shift()
      }
      db[network][service].push({
        ...stats,
        timestamp: secondTimestamp,
      })
    },
    getAll: async (network, service) =>
      Array.from(db[network][service].values()),
    getGroupedByContainer: async network => {
      return Object.values(Container).reduce(
        (accu, current) => ({
          ...accu,
          [current]: [...db[network][current]],
        }),
        {} as Record<Container, StatsDbEntry[]>,
      )
    },
  }
}

export default repositoryFactory
