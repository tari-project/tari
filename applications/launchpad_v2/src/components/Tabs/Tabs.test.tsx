import { act, render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'

import themes from '../../styles/themes'
import Tabs from './'

const tabs = [
  {
    id: 'first-tab',
    content: <span>First tab</span>,
  },
  {
    id: 'second-tab',
    content: <span>Second tab</span>,
  },
]

describe('Tabs', () => {
  it('should render without crashing', async () => {
    const selected = 'second-tab'
    const onSelect = jest.fn()

    await act(async () => {
      render(
        <ThemeProvider theme={themes.light}>
          <Tabs tabs={tabs} selected={selected} onSelect={onSelect} />
        </ThemeProvider>,
      )
    })

    const firstTabText = screen.queryAllByText('First tab')
    expect(firstTabText.length).toBeGreaterThan(0)
  })
})
