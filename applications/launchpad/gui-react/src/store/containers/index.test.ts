import servicesReducer from './'
import { SystemEventAction, ServicesState } from './types'

describe('updateStatus action', () => {
  it('should update container status state', () => {
    // given
    const unsubscribe = jest.fn()
    const state = {
      pending: [],
      containers: {
        someContainerId: {
          status: SystemEventAction.Create,
          stats: {
            cpu: 2,
            memory: 1,
            unsubscribe,
          },
        },
      },
    } as unknown as ServicesState
    const expected = {
      pending: [],
      containers: {
        someContainerId: {
          status: SystemEventAction.Start,
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
      type: 'containers/updateStatus',
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
      pending: [],
      containers: {},
    } as unknown as ServicesState

    // when
    const nextState = servicesReducer(state, {
      type: 'containers/updateStatus',
      payload: {
        containerId: 'newContainerId',
        action: SystemEventAction.Create,
      },
    })

    // then
    const newContainer = nextState.containers.newContainerId
    expect(newContainer).toMatchObject({
      status: SystemEventAction.Create,
      stats: {
        cpu: 0,
        memory: 0,
      },
    })
  })

  it('should add timestamp to the container when adding new', () => {
    // given
    const state = {
      pending: [],
      containers: {},
    } as unknown as ServicesState

    // when
    const nextState = servicesReducer(state, {
      type: 'containers/updateStatus',
      payload: {
        containerId: 'newContainerId',
        action: SystemEventAction.Create,
      },
    })

    // then
    const newContainer = nextState.containers.newContainerId
    expect(newContainer.timestamp).toBeDefined()
  })

  it('should not touch timestamp when updating status', () => {
    // given
    const unsubscribe = jest.fn()
    const state = {
      pending: [],
      containers: {
        someContainerId: {
          timestamp: 123123,
          status: SystemEventAction.Create,
          stats: {
            cpu: 2,
            memory: 1,
            unsubscribe,
          },
        },
      },
    } as unknown as ServicesState

    // when
    const nextState = servicesReducer(state, {
      type: 'containers/updateStatus',
      payload: {
        containerId: 'someContainerId',
        action: SystemEventAction.Start,
      },
    })

    // then
    const newContainer = nextState.containers.someContainerId
    expect(newContainer.timestamp).toBe(123123)
  })

  describe('when container is reported as destroyed or dead', () => {
    const actionsCases = [SystemEventAction.Destroy, SystemEventAction.Die]

    actionsCases.forEach(action => {
      it(`[${action}] should unsubscribe from stats events`, () => {
        // given
        const unsubscribe = jest.fn()
        const state = {
          pending: [],
          containers: {
            someContainerId: {
              status: SystemEventAction.Create,
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
          type: 'containers/updateStatus',
          payload: {
            containerId: 'someContainerId',
            action,
          },
        })

        // then
        expect(unsubscribe).toHaveBeenCalledTimes(1)
      })

      it(`[${action}] should NOT remove container from state`, () => {
        // given
        const state = {
          pending: [],
          containers: {
            someContainerId: {
              status: SystemEventAction.Create,
              stats: {
                cpu: 2,
                memory: 1,
                unsubscribe: jest.fn(),
              },
            },
          },
        } as unknown as ServicesState

        // when
        const nextState = servicesReducer(state, {
          type: 'containers/updateStatus',
          payload: {
            containerId: 'someContainerId',
            action,
          },
        })

        // then
        expect(nextState.containers.someContainerId).toBeDefined()
      })
    })
  })
})
