import { act, fireEvent, render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'

import themes from '../../../../styles/themes'

import MoneroAuthentication from '.'

describe('MoneroAuthentication', () => {
  it('should pass username and password on form submit', async () => {
    const closeFn = jest.fn()
    const onSubmitFn = jest.fn()

    render(
      <ThemeProvider theme={themes.light}>
        <MoneroAuthentication
          defaultValues={undefined}
          onSubmit={onSubmitFn}
          close={closeFn}
        />
      </ThemeProvider>,
    )

    const usernameInput = screen.getByTestId('monero-auth-username-input')
    const passwordInput = screen.getByTestId('monero-auth-password-input')

    await act(async () => {
      fireEvent.input(usernameInput, { target: { value: 'test-username-123' } })
      fireEvent.input(passwordInput, { target: { value: 'test-password-123' } })
    })

    const cancelBtn = screen.getByTestId('monero-auth-submit-btn')
    await act(async () => {
      fireEvent.click(cancelBtn)
    })

    expect(onSubmitFn).toBeCalledTimes(1)
    expect(onSubmitFn).toHaveBeenCalledWith({
      username: 'test-username-123',
      password: 'test-password-123',
    })
  })

  it('should call close function when cancel button is clicked', async () => {
    const closeFn = jest.fn()
    const onSubmitFn = jest.fn()

    render(
      <ThemeProvider theme={themes.light}>
        <MoneroAuthentication
          defaultValues={undefined}
          onSubmit={onSubmitFn}
          close={closeFn}
        />
      </ThemeProvider>,
    )

    const cancelBtn = screen.getByTestId('monero-auth-close-btn')
    act(() => {
      fireEvent.click(cancelBtn)
    })

    expect(closeFn).toBeCalledTimes(1)
  })
})
