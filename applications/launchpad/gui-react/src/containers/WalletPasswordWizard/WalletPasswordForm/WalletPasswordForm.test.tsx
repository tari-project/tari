import { act, fireEvent, render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'

import WalletPasswordForm from '.'

import themes from '../../../styles/themes'

describe('WalletPasswordForm', () => {
  it('should render without crashing when custom submit button text is given', () => {
    const testSubmitBtnText = 'The test text of submit button'
    const submitMock = jest.fn()

    render(
      <ThemeProvider theme={themes.light}>
        <WalletPasswordForm
          onSubmit={submitMock}
          submitBtnText={testSubmitBtnText}
        />
      </ThemeProvider>,
    )
    const el = screen.getByText(testSubmitBtnText)
    expect(el).toBeInTheDocument()
  })

  it('should submit form only if password is set', async () => {
    const testPassword = 'pass'
    const submitMock = jest.fn()

    await act(async () => {
      render(
        <ThemeProvider theme={themes.light}>
          <WalletPasswordForm onSubmit={submitMock} />
        </ThemeProvider>,
      )
    })

    const elInput = screen.getByTestId('password-input')
    expect(elInput).toBeInTheDocument()

    const elSubmitBtn = screen.getByTestId('wallet-password-wizard-submit-btn')
    expect(elSubmitBtn).toBeInTheDocument()

    // Firstly, the form cannot be submitted if password input is empty:
    await act(async () => {
      fireEvent.click(elSubmitBtn)
    })

    expect(submitMock).toHaveBeenCalledTimes(0)

    // Now, set the password...
    await act(async () => {
      fireEvent.input(elInput, { target: { value: testPassword } })
    })

    // ...check the presence of the password in the input...
    expect((elInput as HTMLInputElement).value).toBe(testPassword)

    // ...and submit form again:
    await act(async () => {
      fireEvent.click(elSubmitBtn)
    })

    expect(submitMock).toHaveBeenCalledWith({ password: testPassword })
    expect(submitMock).toHaveBeenCalledTimes(1)
  })
})
