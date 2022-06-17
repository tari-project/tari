import miningReducer from '.'
import { MiningState } from './types'

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
      },
    }
    const expected: MiningState = {
      notifications: [],
      tari: {},
      merged: {
        address: testAddress,
        threads: 1,
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
      },
    }
    const expected: MiningState = {
      tari: {},
      notifications: [],
      merged: {
        address: testAddress,
        threads: testThreads,
        urls: testUrls,
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
})
