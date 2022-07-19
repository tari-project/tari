import { act, renderHook } from '@testing-library/react'

import useAnimationFrame from './useAnimationFrame'

describe('useAnimationFrame', () => {
  let raf: jest.SpyInstance<number, [callback: FrameRequestCallback]>
  let count = 0

  beforeEach(() => {
    // Set up timers and mock requestAnimationFrame
    jest.useFakeTimers()
    raf = jest.spyOn(window, 'requestAnimationFrame')

    raf.mockImplementation((cb: FrameRequestCallback): number => {
      setTimeout(c => cb(c + 1), 100)
      count = count + 1
      return count
    })
  })

  afterEach(() => {
    // Clear mocks and timers
    raf.mockRestore()
    jest.runOnlyPendingTimers()
    jest.useRealTimers()
  })

  it('should call callback at requestAnimationFrame', async () => {
    const cbMock = jest.fn()
    const { result } = renderHook(() => useAnimationFrame(cbMock, true))

    await act(async () => {
      jest.advanceTimersByTime(200)
    })

    expect(count).toBeGreaterThanOrEqual(1)
    expect(result.current.current).toBeGreaterThanOrEqual(1)
    expect(result.current.current).toBeGreaterThanOrEqual(count)
  })
})
