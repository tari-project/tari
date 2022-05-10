import { RootState } from '../'

import { ServiceStatus, Container, SystemEventAction } from './types'
import { selectContainerStatus } from './selectors'

describe('containers/selectors', () => {
  it('should return default state for container if no container of that type is present', () => {
    // given
    const rootState = {
      containers: {
        pending: [],
        containers: {},
      },
    } as unknown as RootState
    const expected = {
      id: '',
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
        pending: [Container.Tor],
        containers: {},
      },
    } as unknown as RootState
    const expected = {
      id: '',
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

  it('should return container', () => {
    // given
    const unsubscribe = jest.fn()
    const rootState = {
      containers: {
        pending: [],
        containers: {
          containerId: {
            id: 'containerId',
            type: Container.Tor,
            lastAction: SystemEventAction.Start,
            stats: {
              cpu: 7,
              memory: 7,
              unsubscribe,
            },
          },
        },
      },
    } as unknown as RootState
    const expected = {
      id: 'containerId',
      lastAction: SystemEventAction.Start,
      type: Container.Tor,
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
        pending: [],
        containers: {
          containerId: {
            id: 'containerId',
            type: Container.Tor,
            lastAction: SystemEventAction.Create,
            stats: {
              cpu: 7,
              memory: 7,
              unsubscribe,
            },
          },
        },
      },
    } as unknown as RootState
    const expected = {
      id: 'containerId',
      type: Container.Tor,
      lastAction: SystemEventAction.Create,
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
