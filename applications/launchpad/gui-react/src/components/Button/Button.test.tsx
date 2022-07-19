import { render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'

import Button from '.'
import SvgAdd from '../../styles/Icons/Add'
import themes from '../../styles/themes'
import styles from '../../styles/styles'

import Text from '../Text'

describe('Button', () => {
  it('should render children wrapped with Text component when children is string', () => {
    const testText = 'The callout test text'
    render(
      <ThemeProvider theme={themes.light}>
        <Button>{testText}</Button>
      </ThemeProvider>,
    )

    const el = screen.getByTestId('button-text-wrapper')
    expect(el).toBeInTheDocument()
  })

  it('should not wrap children when children is ReactNode', () => {
    const testText = <Text>The callout test text</Text>
    render(
      <ThemeProvider theme={themes.light}>
        <Button>{testText}</Button>
      </ThemeProvider>,
    )

    const el = screen.queryByTestId('button-text-wrapper')
    expect(el).not.toBeInTheDocument()
  })

  it('should render icons when icons provided', () => {
    const testText = 'The callout test text'
    render(
      <ThemeProvider theme={themes.light}>
        <Button leftIcon={<SvgAdd />} rightIcon={<SvgAdd />}>
          {testText}
        </Button>
      </ThemeProvider>,
    )

    const elLeft = screen.getByTestId('button-left-icon')
    expect(elLeft).toBeInTheDocument()

    const elRight = screen.getByTestId('button-right-icon')
    expect(elRight).toBeInTheDocument()
  })

  it('should render loading icon when flag loading is set', () => {
    const testText = 'The callout test text'
    render(
      <ThemeProvider theme={themes.light}>
        <Button loading>{testText}</Button>
      </ThemeProvider>,
    )

    const el = screen.getByTestId('button-loading-icon')
    expect(el).toBeInTheDocument()
  })

  it('should render small Text when size is set to small', () => {
    const testText = 'The callout test text'
    render(
      <ThemeProvider theme={themes.light}>
        <Button size='small'>{testText}</Button>
      </ThemeProvider>,
    )

    const el = screen.getByTestId('button-text-wrapper')
    expect(el).toHaveStyle(
      `font-size: ${styles.typography.smallMedium.fontSize}px;`,
    )
  })

  it('should render button in text without crashing', () => {
    const testText = 'The callout test text'
    render(
      <ThemeProvider theme={themes.light}>
        <Button variant='button-in-text' testId='button-in-text'>
          {testText}
        </Button>
      </ThemeProvider>,
    )

    const el = screen.getByTestId('button-in-text')
    expect(el).toBeInTheDocument()
  })

  it('should render button as a link when href prop provided', () => {
    const testText = 'The callout test text'
    const testHref = 'test-href-attr'
    render(
      <ThemeProvider theme={themes.light}>
        <Button href={testHref}>{testText}</Button>
      </ThemeProvider>,
    )

    const el = screen.getByTestId('button-cmp')
    expect(el.nodeName.toLowerCase()).toBe('a')
    expect(el).toHaveAttribute('href', testHref)
  })

  it('should render secondary variant without crashing', () => {
    const testText = 'The callout test text'
    render(
      <ThemeProvider theme={themes.light}>
        <Button variant='secondary'>{testText}</Button>
      </ThemeProvider>,
    )

    const el = screen.getByTestId('button-cmp')
    expect(el).toBeInTheDocument()
  })

  it('should render text variant without crashing', () => {
    const testText = 'The callout test text'
    render(
      <ThemeProvider theme={themes.light}>
        <Button variant='text'>{testText}</Button>
      </ThemeProvider>,
    )

    const el = screen.getByTestId('button-cmp')
    expect(el).toBeInTheDocument()
  })

  it('should render warning variant without crashing', () => {
    const testText = 'The callout test text'
    render(
      <ThemeProvider theme={themes.light}>
        <Button variant='warning'>{testText}</Button>
      </ThemeProvider>,
    )

    const el = screen.getByTestId('button-cmp')
    expect(el).toBeInTheDocument()
  })

  it('should render disabled without crashing', () => {
    const testText = 'The callout test text'
    render(
      <ThemeProvider theme={themes.light}>
        <Button disabled>{testText}</Button>
      </ThemeProvider>,
    )

    const el = screen.getByTestId('button-cmp')
    expect(el).toBeInTheDocument()
  })

  it('should render disabled without crashing when button is an button-in-text', () => {
    const testText = 'The callout test text'
    render(
      <ThemeProvider theme={themes.light}>
        <Button variant='button-in-text' disabled={true}>
          {testText}
        </Button>
      </ThemeProvider>,
    )

    const el = screen.getByTestId('button-cmp')
    expect(el).toBeInTheDocument()
  })
})
