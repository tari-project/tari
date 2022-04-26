import { render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'

import Tag from './'
import SVGCheck from '../../styles/Icons/Check'

import themes from '../../styles/themes'
import styles from '../../styles/styles'
import lightTheme from '../../styles/themes/light'

describe('Tag', () => {
  it('should render Tag component without crashing', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <Tag icon={<SVGCheck />}>Testing</Tag>
      </ThemeProvider>,
    )

    const el = screen.getByText('Testing')
    expect(el).toBeInTheDocument()
  })

  it('should render the correct tag variant', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <Tag icon={<SVGCheck />} variant='large'>
          Testing
        </Tag>
      </ThemeProvider>,
    )

    const largeFontSize = styles.typography.smallHeavy.fontSize

    const el = screen.getByTestId('tag-component')

    expect(el).toHaveStyle(`fontSize: ${largeFontSize}`)
  })

  it('should render the info tag type with style read from the theme', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <Tag icon={<SVGCheck />} type='warning'>
          Testing
        </Tag>
      </ThemeProvider>,
    )

    const warningStyle = lightTheme.warning

    const el = screen.getByTestId('tag-component')
    expect(el).toHaveStyle(`backgroundColor: ${warningStyle}`)
  })

  it('should render the running tag type with style read from the theme', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <Tag icon={<SVGCheck />} type='running'>
          Testing
        </Tag>
      </ThemeProvider>,
    )

    const runningStyle = lightTheme.on

    const el = screen.getByTestId('tag-component')
    expect(el).toHaveStyle(`backgroundColor: ${runningStyle}`)
  })

  it('should render the expert tag type with style read from the theme', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <Tag icon={<SVGCheck />} type='expert'>
          Testing
        </Tag>
      </ThemeProvider>,
    )

    const expertStyle = lightTheme.expert

    const el = screen.getByTestId('tag-component')
    expect(el).toHaveStyle(`backgroundColor: ${expertStyle}`)
  })

  it('should render optional subtext', () => {
    const text = 'hello world'
    render(
      <ThemeProvider theme={themes.light}>
        <Tag subText={text}>Testing</Tag>
      </ThemeProvider>,
    )

    const el = screen.getByText(text)
    expect(el).toBeInTheDocument()
  })

  it('should render optional additional styles from props', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <Tag icon={<SVGCheck />} style={{ backgroundColor: 'red' }}>
          Testing
        </Tag>
      </ThemeProvider>,
    )

    const el = screen.getByTestId('tag-component')
    expect(el).toHaveStyle('backgroundColor: red')
  })
})
