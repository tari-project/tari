import { renderHook } from '@testing-library/react-hooks'

import { Schedule } from '../types/general'
import { clearTime } from '../utils/Date'

import useMiningScheduling from './'

const createPeriodicalGetNow = (start: Date, period: number) => {
  let counter = 0

  return jest.fn(() => {
    const newNow = new Date(start.getTime() + counter++ * period)

    return newNow
  })
}

const A_MINUTE = 60 * 1000

describe('useMiningScheduling', () => {
  it('should run mining according to schedule', () => {
    // given
    const now = new Date('2022-05-23T09:21:00.000Z')
    jest.useFakeTimers().setSystemTime(now)

    const getNow = createPeriodicalGetNow(now, A_MINUTE)
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
})
