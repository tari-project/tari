import servicesReducer from './'
import { Service, SystemEventAction, ServicesState } from './types'

describe('updateStatus action', () => {
  it('should update container lastAction state', () => {
    // given
    const unsubscribe = jest.fn()
    const state = {
      services: {},
      containers: {
        someContainerId: {
          lastAction: SystemEventAction.Create,
          stats: {
            cpu: 2,
            memory: 1,
            unsubscribe,
          },
        },
      },
    } as unknown as ServicesState
    const expected = {
      services: {},
      containers: {
        someContainerId: {
          lastAction: SystemEventAction.Start,
          stats: {
            cpu: 2,
            memory: 1,
            unsubscribe,
          },
        },
      },
    }

    // when
    const nextState = servicesReducer(state, {
      type: 'services/updateStatus',
      payload: {
        containerId: 'someContainerId',
        action: SystemEventAction.Start,
      },
    })

    // then
    expect(nextState).toStrictEqual(expected)
  })

  it('should add the container to state if not present before', () => {
    // given
    const state = {
      services: {},
      containers: {},
    } as unknown as ServicesState
    const expected = {
      services: {},
      containers: {
        newContainerId: {
          lastAction: SystemEventAction.Create,
          stats: {
            cpu: 0,
            memory: 0,
            unsubscribe: () => undefined,
          },
        },
      },
    }

    // when
    const nextState = servicesReducer(state, {
      type: 'services/updateStatus',
      payload: {
        containerId: 'newContainerId',
        action: SystemEventAction.Create,
      },
    })

    // then
    expect(JSON.stringify(nextState)).toStrictEqual(JSON.stringify(expected)) // need to compare like this because () => undefined in initial container state
  })

  describe('when container is reported as destroyed', () => {
    it('should unsubscribe from stats events', () => {
      // given
      const unsubscribe = jest.fn()
      const state = {
        services: {},
        containers: {
          someContainerId: {
            lastAction: SystemEventAction.Create,
            stats: {
              cpu: 2,
              memory: 1,
              unsubscribe,
            },
          },
        },
      } as unknown as ServicesState

      // when
      servicesReducer(state, {
        type: 'services/updateStatus',
        payload: {
          containerId: 'someContainerId',
          action: SystemEventAction.Destroy,
        },
      })

      // then
      expect(unsubscribe).toHaveBeenCalledTimes(1)
    })

    it('should remove relation between container and any service AND reset stats to 0', () => {
      // given
      const state = {
        services: {
          [Service.Tor]: {
            containerId: 'someContainerId',
            pending: false,
          },
        },
        containers: {
          someContainerId: {
            lastAction: SystemEventAction.Create,
            stats: {
              cpu: 2,
              memory: 1,
              unsubscribe: jest.fn(),
            },
          },
        },
      } as unknown as ServicesState
      const expected = {
        services: {
          [Service.Tor]: {
            containerId: '',
            pending: false,
          },
        },
        containers: {
          someContainerId: {
            lastAction: SystemEventAction.Destroy,
            stats: {
              cpu: 0,
              memory: 0,
              unsubscribe: () => undefined,
            },
          },
        },
      }

      // when
      const nextState = servicesReducer(state, {
        type: 'services/updateStatus',
        payload: {
          containerId: 'someContainerId',
          action: SystemEventAction.Destroy,
        },
      })

      // then
      expect(JSON.stringify(nextState)).toBe(JSON.stringify(expected)) // need to check it this way because unsubscribe() gets cleared
    })
  })
})
