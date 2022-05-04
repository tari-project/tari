import { act, render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'

import themes from '../../styles/themes'
import Select from './'

describe('Select', () => {
  it('should render label with select', async () => {
    // given
    const options = [
      {
        value: 'Test value',
        key: 'test',
        label: 'Test label',
      },
    ]

    // when
    await act(async () => {
      render(
        <ThemeProvider theme={themes.light}>
          <Select
            label='Test select label'
            value={options[0]}
            options={options}
            onChange={() => null}
          />
        </ThemeProvider>,
      )
    })

    // then
    const label = screen.getByText(/Test select label/i)
    expect(label).toBeInTheDocument()
  })

  it('should render selected option', async () => {
    // given
    const options = [
      {
        value: 'Test value',
        key: 'test',
        label: 'Test label',
      },
    ]

    // when
    await act(async () => {
      render(
        <ThemeProvider theme={themes.light}>
          <Select
            label='Test select label'
            value={options[0]}
            options={options}
            onChange={() => null}
          />
        </ThemeProvider>,
      )
    })

    // then
    const label = screen.getByText(/Test label/i)
    expect(label).toBeInTheDocument()
  })
})
