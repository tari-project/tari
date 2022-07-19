import { renderHook } from '@testing-library/react-hooks'

import useScheduling from './useScheduling'
import { createPeriodicalGetNow } from './testUtils'

describe('useScheduling', () => {
  it('should call the callback immediately with current time then schedule it for next full minute', async () => {
    const now = new Date('2021-05-21T13:23:11.020Z')
    const millisecondsTillNextMinute = 48980

    jest.useFakeTimers().setSystemTime(now)
    jest.spyOn(global, 'setTimeout')

    const callback = jest.fn()

    renderHook(() =>
      useScheduling({
        callback,
      }),
    )

    expect(callback).toHaveBeenCalledTimes(1)
    expect(setTimeout).toHaveBeenLastCalledWith(
      expect.any(Function),
      millisecondsTillNextMinute,
    )

    jest.advanceTimersByTime(60 * 1000)

    expect(callback).toHaveBeenCalledTimes(2)
  })

  it('should call the callback immediately with current time then schedule it for next full minute on edge second', async () => {
    const now = new Date('2021-05-21T13:23:59.999Z')
    const millisecondsTillNextMinute = 1

    jest.useFakeTimers().setSystemTime(now)
    jest.spyOn(global, 'setTimeout')

    const callback = jest.fn()

    renderHook(() =>
      useScheduling({
        callback,
      }),
    )

    expect(callback).toHaveBeenCalledTimes(1)
    expect(setTimeout).toHaveBeenLastCalledWith(
      expect.any(Function),
      millisecondsTillNextMinute,
    )

    jest.advanceTimersByTime(60 * 1000)

    expect(callback).toHaveBeenCalledTimes(2)
  })

  it('on the full minute should schedule the callback to be run every minute', async () => {
    const now = new Date('2021-05-21T13:23:12.150Z')

    jest.useFakeTimers().setSystemTime(now)
    jest.spyOn(global, 'setTimeout')
    jest.spyOn(global, 'setInterval')

    const callback = jest.fn()

    renderHook(() =>
      useScheduling({
        callback,
      }),
    )

    expect(setInterval).toHaveBeenCalledTimes(0)

    jest.advanceTimersByTime(60 * 1000)
    expect(setInterval).toHaveBeenCalledTimes(1)
    expect(callback).toHaveBeenCalledTimes(2)

    jest.advanceTimersByTime(60 * 1000)
    expect(callback).toHaveBeenCalledTimes(3)

    jest.advanceTimersByTime(60 * 1000)
    expect(callback).toHaveBeenCalledTimes(4)

    jest.advanceTimersByTime(20 * 60 * 1000)
    expect(callback).toHaveBeenCalledTimes(24)
  })

  it('should call the callback with getNow() results', async () => {
    const now = new Date('2021-05-21T13:23:12.150Z')

    jest.useFakeTimers().setSystemTime(now)
    jest.spyOn(global, 'setTimeout')
    jest.spyOn(global, 'setInterval')

    const callback = jest.fn()
    const {
      getNow: getNextMinuteAfterNowOnEveryCall,
      returnedDates: returnedMockedDates,
    } = createPeriodicalGetNow(now, 60 * 1000)

    renderHook(() =>
      useScheduling({
        callback,
        getNow: getNextMinuteAfterNowOnEveryCall,
      }),
    )
    expect(callback).toHaveBeenCalledTimes(1)
    expect(callback).toHaveBeenLastCalledWith(returnedMockedDates[0])

    jest.advanceTimersByTime(60 * 1000)
    expect(callback).toHaveBeenCalledTimes(2)
    expect(callback).toHaveBeenLastCalledWith(returnedMockedDates[1])

    jest.advanceTimersByTime(60 * 1000)
    expect(callback).toHaveBeenCalledTimes(3)
    expect(callback).toHaveBeenLastCalledWith(returnedMockedDates[2])
  })
})
