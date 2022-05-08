import { RootState } from '../'

import { ServiceStatus, Service, SystemEventAction } from './types'
import { selectServiceStatus } from './selectors'

describe('services/selectors', () => {
  it('should return default state for service if no containerId is present', () => {
    // given
    const rootState = {
      services: {
        services: {
          [Service.Tor]: { containerId: '', pending: false },
        },
      },
    } as unknown as RootState
    const expected = {
      running: false,
      pending: false,
      stats: {
        cpu: 0,
        memory: 0,
        unsubscribe: () => undefined,
      },
    }

    // when
    const selected = selectServiceStatus(Service.Tor)(rootState)

    // then
    expect(JSON.stringify(selected)).toBe(JSON.stringify(expected)) // need to check this way because of unsubscribe function
  })

  it('should indicate pending status of service', () => {
    // given
    const rootState = {
      services: {
        services: {
          [Service.Tor]: { containerId: '', pending: true },
        },
      },
    } as unknown as RootState
    const expected = {
      running: false,
      pending: true,
      stats: {
        cpu: 0,
        memory: 0,
        unsubscribe: () => undefined,
      },
    }

    // when
    const selected = selectServiceStatus(Service.Tor)(rootState)

    // then
    expect(JSON.stringify(selected)).toBe(JSON.stringify(expected)) // need to check this way because of unsubscribe function
  })

  it('should return service with assigned containerId as running with stats', () => {
    // given
    const unsubscribe = jest.fn()
    const rootState = {
      services: {
        containers: {
          containerId: {
            lastAction: SystemEventAction.Start,
            stats: {
              cpu: 7,
              memory: 7,
              unsubscribe,
            },
          },
        },
        services: {
          [Service.Tor]: { containerId: 'containerId', pending: false },
        },
      },
    } as unknown as RootState
    const expected = {
      running: true,
      pending: false,
      stats: {
        cpu: 7,
        memory: 7,
        unsubscribe,
      },
    }

    // when
    const selected = selectServiceStatus(Service.Tor)(
      rootState,
    ) as ServiceStatus

    // then
    expect(selected).toStrictEqual(expected) // need to check this way because of unsubscribe function
  })

  it('should return container other than Start or Destroy as pending', () => {
    // given
    const unsubscribe = jest.fn()
    const rootState = {
      services: {
        containers: {
          containerId: {
            lastAction: SystemEventAction.Create,
            stats: {
              cpu: 7,
              memory: 7,
              unsubscribe,
            },
          },
        },
        services: {
          [Service.Tor]: { containerId: 'containerId', pending: false },
        },
      },
    } as unknown as RootState
    const expected = {
      running: true,
      pending: true,
      stats: {
        cpu: 7,
        memory: 7,
        unsubscribe,
      },
    }

    // when
    const selected = selectServiceStatus(Service.Tor)(
      rootState,
    ) as ServiceStatus

    // then
    expect(selected).toStrictEqual(expected) // need to check this way because of unsubscribe function
  })
})

export {}
