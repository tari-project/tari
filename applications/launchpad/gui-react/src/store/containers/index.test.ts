import servicesReducer from './'
import { SystemEventAction, ContainersState } from './types'

describe('store containers slice', () => {
  describe('updateStatus action', () => {
    it('should update container status state', () => {
      // given
      const unsubscribe = jest.fn()
      const state = {
        pending: [],
        containers: {
          someContainerId: {
            status: SystemEventAction.Create,
            exitCode: undefined,
          },
        },
        stats: {
          someContainerId: {
            cpu: 2,
            memory: 1,
            unsubscribe,
          },
        },
      } as unknown as ContainersState
      const expected = {
        pending: [],
        containers: {
          someContainerId: {
            status: SystemEventAction.Start,
            exitCode: undefined,
          },
        },
        stats: {
          someContainerId: {
            cpu: 2,
            memory: 1,
            unsubscribe,
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
        stats: {},
      } as unknown as ContainersState

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
      })
    })

    it('should add timestamp to the container when adding new', () => {
      // given
      const state = {
        pending: [],
        containers: {},
      } as unknown as ContainersState

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
          },
        },
        stats: {
          someContainerId: {
            cpu: 2,
            memory: 1,
            unsubscribe,
          },
        },
      } as unknown as ContainersState

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
              },
            },
            stats: {
              someContainerId: {
                cpu: 2,
                memory: 1,
                unsubscribe,
              },
            },
          } as unknown as ContainersState

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

        it(`[${action}] should NOT remove container from state and 0-out the stats`, () => {
          // given
          const state = {
            pending: [],
            containers: {
              someContainerId: {
                status: SystemEventAction.Create,
              },
            },
            stats: {
              someContainerId: {
                cpu: 2,
                memory: 1,
                unsubscribe: jest.fn(),
              },
            },
          } as unknown as ContainersState

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
          expect(nextState.stats.someContainerId.cpu).toBe(0)
          expect(nextState.stats.someContainerId.memory).toBe(0)
        })
      })
    })
  })
})
