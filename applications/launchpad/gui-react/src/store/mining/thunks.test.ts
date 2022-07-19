import { MiningNodeType } from '../../types/general'
import { RootState } from '..'
import { allStopped } from '../../../__tests__/mocks/states'

import { stopMiningNode, startMiningNode, addMinedTx } from './thunks'
import {
  MiningActionReason,
  TariMiningSetupRequired,
  MergedMiningSetupRequired,
} from './types'

const MINING_NODES = ['tari', 'merged'] as MiningNodeType[]
const REASONS = [MiningActionReason.Manual, MiningActionReason.Schedule]

const shouldNotHaveCalledAnyActions = (
  dispatch: ReturnType<typeof jest.fn>,
  asyncAction: string,
) => {
  expect(dispatch).toHaveBeenCalledTimes(2)
  expect(dispatch.mock.calls[0][0]).toMatchObject({
    payload: undefined,
    type: `mining/${asyncAction}/pending`,
  })
  expect(dispatch.mock.calls[1][0]).toMatchObject({
    payload: undefined,
    type: `mining/${asyncAction}/fulfilled`,
  })
}

const getMockedDispatch = () =>
  jest.fn().mockReturnValue({
    unwrap: () => Promise.resolve(),
  })

describe('starting node', () => {
  it('should throw an error if tari mining start is attempted with misconfigured wallet', async () => {
    // given
    const dispatch = getMockedDispatch()
    const getState = () =>
      ({
        wallet: {
          address: {
            uri: '',
            emoji: '',
          },
          unlocked: false,
        },
        mining: {
          tari: {
            session: undefined,
          },
        },
      } as unknown as RootState)

    // when
    const action = startMiningNode({
      node: 'tari',
      reason: MiningActionReason.Schedule,
    })
    await action(dispatch, getState, undefined)

    // then
    expect(dispatch).toHaveBeenCalledTimes(2)
    expect(dispatch.mock.calls[0][0]).toMatchObject({
      payload: undefined,
      type: 'mining/startNode/pending',
    })
    expect(dispatch.mock.calls[1][0]).toMatchObject({
      payload: TariMiningSetupRequired.MissingWalletAddress,
      type: 'mining/startNode/rejected',
    })
  })

  it('should throw an error if merged mining start is attempted with misconfigured wallet', async () => {
    // given
    const dispatch = getMockedDispatch()
    const getState = () =>
      ({
        wallet: {
          address: {
            uri: '',
            emoji: '',
          },
          unlocked: false,
        },
        mining: {
          merged: {
            address: 'moneroAddress',
            session: undefined,
          },
        },
      } as unknown as RootState)

    // when
    const action = startMiningNode({
      node: 'merged',
      reason: MiningActionReason.Manual,
    })
    await action(dispatch, getState, undefined)

    // then
    expect(dispatch).toHaveBeenCalledTimes(2)
    expect(dispatch.mock.calls[0][0]).toMatchObject({
      payload: undefined,
      type: 'mining/startNode/pending',
    })
    expect(dispatch.mock.calls[1][0]).toMatchObject({
      payload: MergedMiningSetupRequired.MissingWalletAddress,
      type: 'mining/startNode/rejected',
    })
  })

  it('should throw an error if merged mining start is attempted with missing monero address', async () => {
    // given
    const dispatch = getMockedDispatch()
    const getState = () =>
      ({
        wallet: {
          address: {
            uri: 'some wallet adress',
            emoji: '',
          },
          unlocked: true,
        },
        mining: {
          merged: {
            address: undefined,
            session: undefined,
          },
        },
      } as unknown as RootState)

    // when
    const action = startMiningNode({
      node: 'merged',
      reason: MiningActionReason.Manual,
    })
    await action(dispatch, getState, undefined)

    // then
    expect(dispatch).toHaveBeenCalledTimes(2)
    expect(dispatch.mock.calls[0][0]).toMatchObject({
      payload: undefined,
      type: 'mining/startNode/pending',
    })
    expect(dispatch.mock.calls[1][0]).toMatchObject({
      payload: MergedMiningSetupRequired.MissingMoneroAddress,
      type: 'mining/startNode/rejected',
    })
  })
})

describe('start stop mining', () =>
  MINING_NODES.forEach(miningNode =>
    describe(miningNode, () => {
      it(`should not stop ${miningNode} mining by scheduled stop if it was started manually`, async () => {
        // given
        const dispatch = getMockedDispatch()
        const getState = () =>
          ({
            wallet: {
              address: {
                uri: 'walletAddress',
                emoji: '',
              },
              unlocked: true,
            },
            mining: {
              tari: {
                session: {
                  startedAt: new Date('2022-05-23T13:00:00.000Z').getTime(),
                  finishedAt: undefined,
                  reason: MiningActionReason.Manual,
                },
              },
              merged: {
                address: 'moneroAddress',
                session: {
                  startedAt: new Date('2022-05-23T13:00:00.000Z').getTime(),
                  finishedAt: undefined,
                  reason: MiningActionReason.Manual,
                },
              },
            },
          } as unknown as RootState)

        // when
        const action = stopMiningNode({
          node: miningNode,
          reason: MiningActionReason.Schedule,
        })
        await action(dispatch, getState, undefined)

        // then
        shouldNotHaveCalledAnyActions(dispatch, 'stopNode')
      })

      it(`should stop ${miningNode} mining manually after manual start`, async () => {
        // given
        const dispatch = getMockedDispatch()
        const getState = () =>
          ({
            wallet: {
              address: {
                uri: 'some wallet adress',
                emoji: '',
              },
              unlocked: true,
            },
            mining: {
              tari: {
                session: {
                  startedAt: new Date('2022-05-23T13:00:00.000Z').getTime(),
                  finishedAt: undefined,
                  reason: MiningActionReason.Manual,
                },
              },
              merged: {
                address: 'moneroAddress',
                session: {
                  startedAt: new Date('2022-05-23T13:00:00.000Z').getTime(),
                  finishedAt: undefined,
                  reason: MiningActionReason.Manual,
                },
              },
            },
          } as unknown as RootState)

        // when
        const action = stopMiningNode({
          node: miningNode,
          reason: MiningActionReason.Manual,
        })
        await action(dispatch, getState, undefined)

        // then
        expect(dispatch).toHaveBeenCalledWith(
          expect.objectContaining({
            type: 'mining/stopSession',
            payload: { node: miningNode, reason: MiningActionReason.Manual },
          }),
        )
      })

      it(`should stop ${miningNode} mining manually after scheduled start`, async () => {
        // given
        const scheduleId = 'someScheduleId'
        const dispatch = getMockedDispatch()
        const getState = () =>
          ({
            wallet: {
              address: {
                uri: 'some wallet adress',
                emoji: '',
              },
              unlocked: true,
            },
            mining: {
              tari: {
                session: {
                  startedAt: new Date('2022-05-23T13:00:00.000Z').getTime(),
                  finishedAt: undefined,
                  reason: MiningActionReason.Schedule,
                  schedule: scheduleId,
                },
              },
              merged: {
                address: 'moneroAddress',
                session: {
                  startedAt: new Date('2022-05-23T13:00:00.000Z').getTime(),
                  finishedAt: undefined,
                  reason: MiningActionReason.Schedule,
                  schedule: scheduleId,
                },
              },
            },
          } as unknown as RootState)

        // when
        const action = stopMiningNode({
          node: miningNode,
          reason: MiningActionReason.Manual,
        })
        await action(dispatch, getState, undefined)

        // then
        expect(dispatch).toHaveBeenCalledWith(
          expect.objectContaining({
            type: 'mining/stopSession',
            payload: { node: miningNode, reason: MiningActionReason.Manual },
          }),
        )
      })

      REASONS.forEach(previousSessionStopReason => {
        it(`should start ${miningNode} mining manually for stopReason: ${previousSessionStopReason}`, async () => {
          // given
          const dispatch = getMockedDispatch()
          const getState = () =>
            ({
              wallet: {
                address: {
                  uri: 'some wallet adress',
                  emoji: '',
                },
                unlocked: true,
              },
              containers: allStopped,
              mining: {
                tari: {
                  session: {
                    startedAt: new Date('2022-05-23T13:00:00.000Z').getTime(),
                    finishedAt: new Date('2022-05-23T19:00:00.000Z').getTime(),
                    reason: previousSessionStopReason,
                  },
                },
                merged: {
                  address: 'moneroAddress',
                  session: {
                    startedAt: new Date('2022-05-23T13:00:00.000Z').getTime(),
                    finishedAt: new Date('2022-05-23T19:00:00.000Z').getTime(),
                    reason: previousSessionStopReason,
                  },
                },
              },
            } as unknown as RootState)

          // when
          const action = startMiningNode({
            node: miningNode,
            reason: MiningActionReason.Manual,
          })
          await action(dispatch, getState, undefined)

          // then
          expect(dispatch).toHaveBeenCalledWith(
            expect.objectContaining({
              type: 'mining/startNewSession',
              payload: {
                node: miningNode,
                reason: MiningActionReason.Manual,
              },
            }),
          )
        })

        it(`should start ${miningNode} mining on schedule for previous stop reason: ${previousSessionStopReason}`, async () => {
          // given
          const scheduleId = 'someScheduleId'
          const dispatch = getMockedDispatch()
          const getState = () =>
            ({
              wallet: {
                address: {
                  uri: 'some wallet adress',
                  emoji: '',
                },
                unlocked: true,
              },
              containers: allStopped,
              mining: {
                tari: {
                  session: {
                    startedAt: new Date('2022-05-23T13:00:00.000Z').getTime(),
                    finishedAt: new Date('2022-05-23T19:00:00.000Z').getTime(),
                    reason: previousSessionStopReason,
                    schedule:
                      previousSessionStopReason === MiningActionReason.Schedule
                        ? scheduleId
                        : undefined,
                  },
                },
                merged: {
                  address: 'moneroAddress',
                  session: {
                    startedAt: new Date('2022-05-23T13:00:00.000Z').getTime(),
                    finishedAt: new Date('2022-05-23T19:00:00.000Z').getTime(),
                    reason: previousSessionStopReason,
                    schedule:
                      previousSessionStopReason === MiningActionReason.Schedule
                        ? scheduleId
                        : undefined,
                  },
                },
              },
            } as unknown as RootState)

          // when
          const action = startMiningNode({
            node: miningNode,
            reason: MiningActionReason.Schedule,
            schedule: scheduleId,
          })
          await action(dispatch, getState, undefined)

          // then
          expect(dispatch).toHaveBeenCalledWith(
            expect.objectContaining({
              type: 'mining/startNewSession',
              payload: {
                node: miningNode,
                reason: MiningActionReason.Schedule,
                schedule: scheduleId,
              },
            }),
          )
        })
      })

      it(`should not start manually stopped scheduled ${miningNode} mining until next schedule`, async () => {
        // given
        const previousScheduleId = 'previousScheduleId'
        const dispatch = getMockedDispatch()
        const getState = () =>
          ({
            wallet: {
              address: {
                uri: 'some wallet adress',
                emoji: '',
              },
              unlocked: true,
            },
            containers: allStopped,
            mining: {
              tari: {
                session: {
                  startedAt: new Date('2022-05-23T13:00:00.000Z').getTime(),
                  finishedAt: new Date('2022-05-23T19:00:00.000Z').getTime(),
                  schedule: previousScheduleId,
                  reason: MiningActionReason.Manual,
                },
              },
              merged: {
                address: 'moneroAddress',
                session: {
                  startedAt: new Date('2022-05-23T13:00:00.000Z').getTime(),
                  finishedAt: new Date('2022-05-23T19:00:00.000Z').getTime(),
                  schedule: previousScheduleId,
                  reason: MiningActionReason.Manual,
                },
              },
            },
          } as unknown as RootState)

        // when
        const action = startMiningNode({
          node: miningNode,
          reason: MiningActionReason.Schedule,
          schedule: previousScheduleId,
        })
        await action(dispatch, getState, undefined)

        shouldNotHaveCalledAnyActions(dispatch, 'startNode')
      })

      it(`should start manually stopped scheduled ${miningNode} mining if different schedule starts it`, async () => {
        // given
        const previousScheduleId = 'previousScheduleId'
        const nextScheduleId = 'nextScheduleId'
        const dispatch = getMockedDispatch()
        const getState = () =>
          ({
            wallet: {
              address: {
                uri: 'some wallet adress',
                emoji: '',
              },
              unlocked: true,
            },
            containers: allStopped,
            mining: {
              tari: {
                session: {
                  startedAt: new Date('2022-05-23T13:00:00.000Z').getTime(),
                  finishedAt: new Date('2022-05-23T19:00:00.000Z').getTime(),
                  schedule: previousScheduleId,
                  reason: MiningActionReason.Manual,
                },
              },
              merged: {
                address: 'moneroAddress',
                session: {
                  startedAt: new Date('2022-05-23T13:00:00.000Z').getTime(),
                  finishedAt: new Date('2022-05-23T19:00:00.000Z').getTime(),
                  schedule: previousScheduleId,
                  reason: MiningActionReason.Manual,
                },
              },
            },
          } as unknown as RootState)

        // when
        const action = startMiningNode({
          node: miningNode,
          reason: MiningActionReason.Schedule,
          schedule: nextScheduleId,
        })
        await action(dispatch, getState, undefined)

        // next
        expect(dispatch).toHaveBeenCalledWith(
          expect.objectContaining({
            type: 'mining/startNewSession',
            payload: {
              node: miningNode,
              reason: MiningActionReason.Schedule,
              schedule: nextScheduleId,
            },
          }),
        )
      })
    }),
  ))

describe('Mining events', () => {
  it('should call addMined slice when data is correct', async () => {
    // given
    const dispatch = getMockedDispatch()
    const getState = () =>
      ({
        wallet: {
          unlocked: true,
        },
        mining: {
          tari: {
            session: { total: { xtr: 0 }, history: [] },
          },
        },
      } as unknown as RootState)

    // when
    const action = addMinedTx({
      amount: 100,
      node: 'tari',
      txId: 'test-tx-id-1',
    })
    const result = await action(dispatch, getState, undefined)

    // then
    expect(result.type.endsWith('/fulfilled')).toBe(true)
  })

  it('should not call addMined action when transaction is already on the list', async () => {
    // given
    const dispatch = getMockedDispatch()
    const getState = () =>
      ({
        wallet: {
          unlocked: true,
        },
        mining: {
          tari: {
            session: {
              total: { xtr: 0 },
              history: [
                {
                  txId: 'test-tx-id-1',
                  amount: 100,
                },
              ],
            },
          },
        },
      } as unknown as RootState)

    // when
    const action = addMinedTx({
      amount: 100,
      node: 'tari',
      txId: 'test-tx-id-1',
    })
    const result = await action(dispatch, getState, undefined)

    // then
    expect(result.type.endsWith('/rejected')).toBe(true)
  })
})
