// db.ts
import Dexie, { Table } from 'dexie'

import { StatsEntry } from './store/containers/statsRepository'

export class TariSubclassedDexie extends Dexie {
  // 'stats' is added by dexie when declaring the stores()
  // We just tell the typing system this is the case so it does not complain
  // and gives us suggestions
  stats!: Table<StatsEntry>

  constructor() {
    super('tariDatabase')
    this.version(1).stores({
      stats: '[timestamp+network], network, container',
    })
  }
}

export const db = new TariSubclassedDexie()
