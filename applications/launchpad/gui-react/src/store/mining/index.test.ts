import miningReducer from '.'
import { MiningActionReason, MiningState } from './types'

describe('Mining Redux slice', () => {
  it('should set merged address', () => {
    // given
    const testAddress = 'testAddress'
    const state: MiningState = {
      notifications: [],
      tari: {},
      merged: {
        address: undefined,
        threads: 1,
        useAuth: false,
      },
    }
    const expected: MiningState = {
      notifications: [],
      tari: {},
      merged: {
        address: testAddress,
        threads: 1,
        useAuth: false,
      },
    }

    // when
    const nextState = miningReducer(state, {
      type: 'mining/setMergedAddress',
      payload: { address: testAddress },
    })

    // then
    expect(nextState).toStrictEqual(expected)
  })

  it('should set merged config', () => {
    // given
    const testAddress = 'testAddress'
    const testThreads = 3
    const testUrls = [{ url: 'some-url-1' }, { url: 'some-url-2' }]

    const state: MiningState = {
      notifications: [],
      tari: {},
      merged: {
        address: undefined,
        threads: 1,
        urls: [],
        useAuth: false,
      },
    }
    const expected: MiningState = {
      tari: {},
      notifications: [],
      merged: {
        address: testAddress,
        threads: testThreads,
        urls: testUrls,
        useAuth: false,
      },
    }

    // when
    const nextState = miningReducer(state, {
      type: 'mining/setMergedConfig',
      payload: {
        address: testAddress,
        threads: testThreads,
        urls: testUrls,
      },
    })

    // then
    expect(nextState).toStrictEqual(expected)
  })

  it('should add amount to the Tari session', () => {
    // given
    const state: MiningState = {
      tari: {
        session: {
          total: {
            xtr: 0,
          },
          history: [],
          reason: MiningActionReason.Manual,
        },
      },
      merged: {
        address: undefined,
        threads: 1,
        urls: [],
        useAuth: false,
      },
      notifications: [],
    }
    const expected: MiningState = {
      tari: {
        session: {
          total: {
            xtr: 1000,
          },
          history: [
            {
              txId: 'tx-id',
              amount: 1000,
            },
          ],
          reason: MiningActionReason.Manual,
        },
      },
      merged: {
        address: undefined,
        threads: 1,
        urls: [],
        useAuth: false,
      },

      notifications: [],
    }

    // when
    const nextState = miningReducer(state, {
      type: 'mining/addMinedTx/fulfilled',
      payload: {
        amount: 1000,
        txId: 'tx-id',
        node: 'tari',
      },
    })

    // then
    expect(nextState).toStrictEqual(expected)
  })
})
