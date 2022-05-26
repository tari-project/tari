import { renderHook } from '@testing-library/react-hooks'

import { Schedule } from '../types/general'
import { createPeriodicalGetNow } from '../utils/testUtils'
import { startOfUTCDay } from '../utils/Date'

import useMiningScheduling from './useMiningScheduling'

const A_MINUTE = 60 * 1000

describe('useMiningScheduling', () => {
  it('should run mining according to schedule', () => {
    // given
    const now = new Date('2022-05-23T09:20:00.000Z')
    jest.useFakeTimers().setSystemTime(now)

    const { getNow } = createPeriodicalGetNow(now, A_MINUTE)
    const startMining = jest.fn()
    const stopMining = jest.fn()
    const schedules = [
      {
        id: 'shortSchedule',
        enabled: true,
        date: startOfUTCDay(now),
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
    expect(startMining).toHaveBeenCalledWith('tari', 'shortSchedule')
    expect(startMining).toHaveBeenCalledWith('merged', 'shortSchedule')

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

    const { getNow } = createPeriodicalGetNow(now, A_MINUTE)
    const schedules = [
      {
        id: 'shortSchedule',
        enabled: true,
        date: startOfUTCDay(dayFromNow),
        interval: {
          from: {
            hours: 9,
            minutes: 24,
          },
          to: {
            hours: 9,
            minutes: 25,
          },
        },
        type: ['tari', 'merged'],
      },
    ] as Schedule[]

    // when
    renderHook(() =>
      useMiningScheduling({
        schedules,
        stopMining: () => null,
        startMining: () => null,
        getNow,
      }),
    )

    // then
    expect(setTimeout).toHaveBeenCalledWith(
      expect.any(Function),
      24 * 60 * A_MINUTE,
    )
  })
})
