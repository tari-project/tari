import { MiningState } from '../../../src/store/mining/types'

export const initialMining: MiningState = {
  tari: {
    session: undefined,
  },
  merged: {
    session: undefined,
  },
}

export const miningWithSessions: MiningState = {
  tari: {
    session: {
      startedAt: Number(Date.now()).toString(),
      total: {
        xtr: '1000',
      },
    },
  },
  merged: {
    threads: 1,
    urls: ['firstAddress'],
    address: 'address',
    session: {
      startedAt: Number(Date.now()).toString(),
      total: {
        xtr: '1000',
        xmr: '1001',
      },
    },
  },
}
