import { act, fireEvent, render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'

import RunningButton from '.'
import themes from '../../styles/themes'

describe('RunningButton', () => {
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

  it('should count time when is active', async () => {
    const cbMock = jest.fn()

    // Render the Timer
    await act(async () => {
      render(
        <ThemeProvider theme={themes.light}>
          <RunningButton
            startedAt={Number(Date.now())}
            onClick={cbMock}
            active={true}
          />
        </ThemeProvider>,
      )
    })

    // Check that the timer has zeros only at the beginning
    let timerEl = screen.getByTestId('timer-test-id')
    expect(timerEl.textContent).toBe('0:00:00')

    // move the clock at least by 1 sec
    await act(async () => {
      jest.advanceTimersByTime(1200)
    })

    jest.clearAllTimers()

    // Now the timer should be changed
    timerEl = screen.getByTestId('timer-test-id')
    expect(timerEl.textContent).toBe('0:00:01')
  })

  it('should call onClick when clicked', async () => {
    const cbMock = jest.fn()

    render(
      <ThemeProvider theme={themes.light}>
        <RunningButton
          startedAt={Number(Date.now())}
          onClick={cbMock}
          active={true}
        />
      </ThemeProvider>,
    )

    const elBtn = screen.getByTestId('running-button-cmp')
    await act(async () => {
      fireEvent.click(elBtn)
    })

    expect(cbMock).toBeCalled()
  })
})
