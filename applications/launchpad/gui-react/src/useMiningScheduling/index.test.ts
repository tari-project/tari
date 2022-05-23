import { renderHook, act } from '@testing-library/react-hooks'

import { Schedule } from '../types/general'
import { clearTime } from '../utils/Date'

import useMiningScheduling from './'

const createPeriodicalGetNow = (start: Date, period: number) => {
  let from = new Date(start)
  let counter = 0

  const getNow = jest.fn(() => {
    const newNow = new Date(from.getTime() + counter++ * period)

    return newNow
  })

  const reset = (d: Date) => {
    from = new Date(d)
    counter = 0
  }

  return { getNow, reset }
}

const A_MINUTE = 60 * 1000

describe('useMiningScheduling', () => {
  it('should run mining according to schedule', () => {
    // given
    const now = new Date('2022-05-23T09:21:00.000Z')
    jest.useFakeTimers().setSystemTime(now)

    const { getNow } = createPeriodicalGetNow(now, A_MINUTE)
    const startMining = jest.fn()
    const stopMining = jest.fn()
    const schedules = [
      {
        id: 'shortSchedule',
        enabled: true,
        date: clearTime(now),
        interval: {
          from: {
            hours: 9,
            minutes: 22,
          },
          to: {
            hours: 9,
            minutes: 23,
          },
        },
        type: ['tari', 'merged'],
      },
    ] as Schedule[]

    // when
    renderHook(() =>
      useMiningScheduling({
        schedules,
        stopMining,
        startMining,
        getNow,
      }),
    )

    // then
    jest.advanceTimersByTime(A_MINUTE + 1)
    expect(startMining).toHaveBeenCalledTimes(2)
    expect(startMining).toHaveBeenCalledWith('tari')
    expect(startMining).toHaveBeenCalledWith('merged')

    jest.advanceTimersByTime(A_MINUTE)
    expect(stopMining).toHaveBeenCalledTimes(2)
    expect(stopMining).toHaveBeenCalledWith('tari')
    expect(stopMining).toHaveBeenCalledWith('merged')
  })

  it('should schedule mining for schedules 24h + from now', () => {
    // given
    const now = new Date('2022-05-23T09:21:00.000Z')
    const dayFromNow = new Date('2022-05-24T09:21:00.000Z')
    jest.useFakeTimers().setSystemTime(now)
    jest.spyOn(global, 'setTimeout')

    const { getNow, reset: resetGetNow } = createPeriodicalGetNow(now, A_MINUTE)
    const startMining = jest.fn()
    const stopMining = jest.fn()
    const schedules = [
      {
        id: 'shortSchedule',
        enabled: true,
        date: clearTime(dayFromNow),
        interval: {
          from: {
            hours: 9,
            minutes: 22,
          },
          to: {
            hours: 9,
            minutes: 23,
          },
        },
        type: ['tari', 'merged'],
      },
    ] as Schedule[]

    // when
    renderHook(() =>
      useMiningScheduling({
        schedules,
        stopMining,
        startMining,
        getNow,
      }),
    )

    // then
    expect(setTimeout).toHaveBeenCalledWith(
      expect.any(Function),
      24 * 60 * 60 * 1000,
    )
    act(() => {
      jest.advanceTimersByTime(24 * 60 * A_MINUTE + 1)
      resetGetNow(dayFromNow)
    })
    act(() => {
      jest.advanceTimersByTime(A_MINUTE + 1)
      expect(startMining).toHaveBeenCalledTimes(2)
      expect(startMining).toHaveBeenCalledWith('tari')
      expect(startMining).toHaveBeenCalledWith('merged')

      jest.advanceTimersByTime(A_MINUTE + 1)
      expect(stopMining).toHaveBeenCalledTimes(2)
      expect(stopMining).toHaveBeenCalledWith('tari')
      expect(stopMining).toHaveBeenCalledWith('merged')
    })
  })
})
