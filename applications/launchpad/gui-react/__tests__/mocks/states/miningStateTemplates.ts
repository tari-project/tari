import { MiningState } from '../../../src/store/mining/types'

export const initialMining: MiningState = {
  tari: {
    sessions: [],
  },
  merged: {
    sessions: [],
  },
}

export const miningWithSessions: MiningState = {
  tari: {
    sessions: [
      {
        total: {
          xtr: '1000',
        },
      },
      {
        total: {
          xtr: '2000',
        },
      },
    ],
  },
  merged: {
    threads: 1,
    urls: ['firstAddress'],
    address: 'address',
    sessions: [
      {
        total: {
          xtr: '1000',
          xmr: '1001',
        },
      },
      {
        total: {
          xtr: '2000',
          xmr: '2001',
        },
      },
    ],
  },
}
