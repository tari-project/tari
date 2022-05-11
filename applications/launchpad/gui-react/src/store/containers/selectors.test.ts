import { RootState } from '../'

import { ContainerStatusDto, Container, SystemEventAction } from './types'
import { selectContainerStatus } from './selectors'

describe('containers/selectors', () => {
  it('should return default state for container if no container of that type is present', () => {
    // given
    const rootState = {
      containers: {
        errors: {},
        pending: [],
        containers: {},
      },
    } as unknown as RootState
    const expected = {
      id: '',
      type: Container.Tor,
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
        errors: {},
        pending: [Container.Tor],
        containers: {},
      },
    } as unknown as RootState
    const expected = {
      id: '',
      type: Container.Tor,
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

  it('should return container by type', () => {
    // given
    const unsubscribe = jest.fn()
    const rootState = {
      containers: {
        errors: {},
        pending: [],
        containers: {
          containerId: {
            type: Container.Tor,
            status: SystemEventAction.Start,
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
      status: SystemEventAction.Start,
      type: Container.Tor,
      error: undefined,
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
    ) as ContainerStatusDto

    // then
    expect(selected).toStrictEqual(expected) // need to check this way because of unsubscribe function
  })

  it('should return prioritize local error over container-type-errors', () => {
    // given
    const containerError = { some: 'error' }
    const containerTypeError = { some: 'other error' }
    const rootState = {
      containers: {
        errors: {
          [Container.Tor]: containerTypeError,
        },
        pending: [],
        containers: {
          containerId: {
            type: Container.Tor,
            error: containerError,
            status: SystemEventAction.Start,
            stats: {
              cpu: 7,
              memory: 7,
            },
          },
        },
      },
    } as unknown as RootState

    // when
    const selectedContainer = selectContainerStatus(Container.Tor)(
      rootState,
    ) as ContainerStatusDto

    // then
    expect(selectedContainer.error).toBe(containerError)
  })

  it('should return container by type with error if present', () => {
    // given
    const containerTypeError = { some: 'error' }
    const rootState = {
      containers: {
        errors: {
          [Container.Tor]: containerTypeError,
        },
        pending: [],
        containers: {
          containerId: {
            type: Container.Tor,
            status: SystemEventAction.Start,
            stats: {
              cpu: 7,
              memory: 7,
            },
          },
        },
      },
    } as unknown as RootState

    // when
    const selectedContainer = selectContainerStatus(Container.Tor)(
      rootState,
    ) as ContainerStatusDto

    // then
    expect(selectedContainer.error).toBe(containerTypeError)
  })

  const runningIndicationTestcases = [
    [SystemEventAction.Start, true],
    [SystemEventAction.Destroy, false],
    [SystemEventAction.Die, false],
    ['something else', false],
  ]
  runningIndicationTestcases.forEach(([status, expected]) =>
    it(`should return running=${expected} for status "${status}"`, () => {
      // given
      const unsubscribe = jest.fn()
      const rootState = {
        containers: {
          errors: {},
          pending: [],
          containers: {
            containerId: {
              type: Container.Tor,
              status: status,
              stats: {
                cpu: 7,
                memory: 7,
                unsubscribe,
              },
            },
          },
        },
      } as unknown as RootState

      // when
      const selected = selectContainerStatus(Container.Tor)(
        rootState,
      ) as ContainerStatusDto

      // then
      expect(selected.running).toBe(expected)
    }),
  )

  it('should return container with biggest timestamp value if multiple containers of the same type are present', () => {
    // given
    const unsubscribe = jest.fn()
    const rootState = {
      containers: {
        errors: {},
        pending: [],
        containers: {
          containerId: {
            timestamp: 0,
            type: Container.Tor,
            status: SystemEventAction.Start,
            stats: {
              cpu: 7,
              memory: 7,
              unsubscribe,
            },
          },
          anotherContainerId: {
            timestamp: 1,
            type: Container.Tor,
            status: SystemEventAction.Start,
            stats: {
              cpu: 8,
              memory: 8,
              unsubscribe,
            },
          },
        },
      },
    } as unknown as RootState
    const expected = {
      id: 'anotherContainerId',
      timestamp: 1,
      status: SystemEventAction.Start,
      type: Container.Tor,
      error: undefined,
      running: true,
      pending: false,
      stats: {
        cpu: 8,
        memory: 8,
        unsubscribe,
      },
    }

    // when
    const selected = selectContainerStatus(Container.Tor)(
      rootState,
    ) as ContainerStatusDto

    // then
    expect(selected).toStrictEqual(expected) // need to check this way because of unsubscribe function
  })

  it('should return container other than Start or Destroy as pending', () => {
    // given
    const unsubscribe = jest.fn()
    const rootState = {
      containers: {
        errors: {},
        pending: [],
        containers: {
          containerId: {
            type: Container.Tor,
            status: SystemEventAction.Create,
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
      status: SystemEventAction.Create,
      error: undefined,
      running: false,
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
    ) as ContainerStatusDto

    // then
    expect(selected).toStrictEqual(expected) // need to check this way because of unsubscribe function
  })
})
