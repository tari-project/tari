import { RootState } from '..'

import { addStats } from './thunks'
import { SystemEventAction, Container } from './types'

describe('containers/thunks', () => {
  describe('containers/stats', () => {
    const standardTimestamp = '2022-05-23T12:13:14.000Z'
    const standardStats = (read = standardTimestamp) => ({
      read,
      precpu_stats: {
        cpu_usage: {
          total_usage: 1,
        },
        system_cpu_usage: 1,
      },
      cpu_stats: {
        cpu_usage: {
          total_usage: 3,
        },
        system_cpu_usage: 5,
        online_cpus: 1,
      },
      memory_stats: {
        usage: 1024 * 1024,
        stats: {},
      },
      networks: {},
    })
    const expectedFromStandardStats = () => ({
      cpu: 50,
      memory: 1,
    })

    it('should return stats to be saved in state', async () => {
      // given
      const expectedNetwork = 'someNetwork'
      const expectedContainerId = 'someContainerId'
      const getState = () =>
        ({
          baseNode: {
            network: expectedNetwork,
          },
          containers: {
            statsHistory: [],
            pending: [],
            containers: {
              [expectedContainerId]: {
                status: SystemEventAction.Start,
              },
            },
            stats: {
              [expectedContainerId]: {
                cpu: 0,
                memory: 0,
              },
            },
          },
        } as unknown as RootState)

      // when
      const action = addStats({
        containerId: expectedContainerId,
        service: Container.Tor,
        stats: standardStats(),
      })
      const result = await action(() => null, getState, undefined)

      // then
      expect(result.type.endsWith('/fulfilled')).toBe(true)
      expect(result.payload).toStrictEqual({
        containerId: expectedContainerId,
        stats: expectedFromStandardStats(),
      })
    })

    it('should reject if stats for containerId do not exist yet', async () => {
      // given
      const getState = () =>
        ({
          baseNode: {
            network: 'dibbler',
          },
          containers: {
            statsHistory: [],
            pending: [],
            containers: {
              someContainerId: {
                status: SystemEventAction.Start,
              },
            },
            stats: {},
          },
        } as unknown as RootState)

      // when
      const action = addStats({
        containerId: 'someContainerId',
        service: Container.Tor,
        stats: standardStats(),
      })
      const result = await action(() => null, getState, undefined)

      // then
      expect(result.type.endsWith('/rejected')).toBe(true)
    })
  })
})
