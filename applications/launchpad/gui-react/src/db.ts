// db.ts
import Dexie, { Table } from 'dexie'

import { Container } from './store/containers/types'

export interface StatsEntry {
  timestamp: string
  network: string
  service: Container
  cpu: number
  memory: number
  upload: number
  download: number
}

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
