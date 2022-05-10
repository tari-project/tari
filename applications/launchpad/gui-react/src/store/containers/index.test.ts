import servicesReducer from './'
import { SystemEventAction, ServicesState } from './types'

describe('updateStatus action', () => {
  it('should update container lastAction state', () => {
    // given
    const unsubscribe = jest.fn()
    const state = {
      pending: [],
      containers: {
        someContainerId: {
          id: 'someContainerId',
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
      pending: [],
      containers: {
        someContainerId: {
          id: 'someContainerId',
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
    const expected = {
      pending: [],
      containers: {
        newContainerId: {
          id: 'newContainerId',
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
      type: 'containers/updateStatus',
      payload: {
        containerId: 'newContainerId',
        action: SystemEventAction.Create,
      },
    })

    // then
    expect(JSON.stringify(nextState)).toStrictEqual(JSON.stringify(expected)) // need to compare like this because () => undefined in initial container state
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
              id: 'someContainerId',
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
          type: 'containers/updateStatus',
          payload: {
            containerId: 'someContainerId',
            action,
          },
        })

        // then
        expect(unsubscribe).toHaveBeenCalledTimes(1)
      })

      it(`[${action}] should remove container from state`, () => {
        // given
        const state = {
          pending: [],
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
          pending: [],
          containers: {},
        }

        // when
        const nextState = servicesReducer(state, {
          type: 'containers/updateStatus',
          payload: {
            containerId: 'someContainerId',
            action,
          },
        })

        // then
        expect(JSON.stringify(nextState)).toBe(JSON.stringify(expected)) // need to check it this way because unsubscribe() gets cleared
      })
    })
  })
})
