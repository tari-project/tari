import { render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'

import themes from '../../styles/themes'
import Tag from './'
import SVGCheck from '../../styles/Icons/Check'

describe('Tag', () => {
  it('should render Tag component without crashing', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <Tag icon={<SVGCheck />}>Testing</Tag>
      </ThemeProvider>,
    )

    const el = screen.getByText('Testing')
    expect(el).toBeInTheDocument()
    expect(el).toHaveStyle('fontSize: 12')
  })

  it('should render the correct tag variant', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <Tag icon={<SVGCheck />} variant='large'>
          Testing
        </Tag>
      </ThemeProvider>,
    )

    const el = screen.getByText('Testing')
    expect(el).toHaveStyle('fontSize: 14')
  })

  it('should render optional subtext', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <Tag subText='hello world'>Testing</Tag>
      </ThemeProvider>,
    )

    const el = screen.getByText('hello world')
    expect(el).toBeInTheDocument()
  })
})
