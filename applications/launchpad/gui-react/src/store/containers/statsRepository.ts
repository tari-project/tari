import { StatsDbEntry, StatsRepository, Container } from './types'

const db: Record<string, Record<Container, Map<string, StatsDbEntry>>> = {}

const ensureDb = (network: string) => {
  if (!db[network]) {
    db[network] = {
      [Container.Tor]: new Map<string, StatsDbEntry>(),
      [Container.BaseNode]: new Map<string, StatsDbEntry>(),
      [Container.Wallet]: new Map<string, StatsDbEntry>(),
      [Container.SHA3Miner]: new Map<string, StatsDbEntry>(),
      [Container.MMProxy]: new Map<string, StatsDbEntry>(),
      [Container.XMrig]: new Map<string, StatsDbEntry>(),
      [Container.Monerod]: new Map<string, StatsDbEntry>(),
      [Container.Frontail]: new Map<string, StatsDbEntry>(),
    }
  }
}

// TODO implement actual db https://github.com/Altalogy/tari/issues/46
// THIS IS A TEMPORARY IMPLEMENTATION
// this is the dumbest implementation I could write
// future implementation will use Dexie.js
const repositoryFactory: () => StatsRepository = () => {
  return {
    add: async (network, service, secondTimestamp, stats) => {
      ensureDb(network)

      db[network][service].set(secondTimestamp, {
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
          [current]: Array.from(db[network][current].values()),
        }),
        {} as Record<Container, StatsDbEntry[]>,
      )
    },
  }
}

export default repositoryFactory
