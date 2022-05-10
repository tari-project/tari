import { RootState } from '../'

import { ServiceStatus, Container, SystemEventAction } from './types'
import { selectContainerStatus } from './selectors'

describe('services/selectors', () => {
  it('should return default state for service if no containerId is present', () => {
    // given
    const rootState = {
      containers: {
        services: {
          [Container.Tor]: { containerId: '', pending: false },
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
    const selected = selectContainerStatus(Container.Tor)(rootState)

    // then
    expect(JSON.stringify(selected)).toBe(JSON.stringify(expected)) // need to check this way because of unsubscribe function
  })

  it('should indicate pending status of service', () => {
    // given
    const rootState = {
      containers: {
        services: {
          [Container.Tor]: { containerId: '', pending: true },
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
    const selected = selectContainerStatus(Container.Tor)(rootState)

    // then
    expect(JSON.stringify(selected)).toBe(JSON.stringify(expected)) // need to check this way because of unsubscribe function
  })

  it('should return service with assigned containerId as running with stats', () => {
    // given
    const unsubscribe = jest.fn()
    const rootState = {
      containers: {
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
          [Container.Tor]: { containerId: 'containerId', pending: false },
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
    const selected = selectContainerStatus(Container.Tor)(
      rootState,
    ) as ServiceStatus

    // then
    expect(selected).toStrictEqual(expected) // need to check this way because of unsubscribe function
  })

  it('should return container other than Start or Destroy as pending', () => {
    // given
    const unsubscribe = jest.fn()
    const rootState = {
      containers: {
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
          [Container.Tor]: { containerId: 'containerId', pending: false },
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
    const selected = selectContainerStatus(Container.Tor)(
      rootState,
    ) as ServiceStatus

    // then
    expect(selected).toStrictEqual(expected) // need to check this way because of unsubscribe function
  })
})

export {}
