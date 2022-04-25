import { fireEvent, render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'

import Switch from '.'
import themes from '../../styles/themes'

describe('Switch', () => {
  it('should render without crash', () => {
    const onClick = jest.fn()
    const testLabel = 'Test label for the switch component'
    const anotherTestLabel = 'Test label for the switch component'
    const val = false
    render(
      <ThemeProvider theme={themes.light}>
        <Switch
          value={val}
          leftLabel={testLabel}
          rightLabel={anotherTestLabel}
          onClick={onClick}
        />
      </ThemeProvider>,
    )

    const el = screen.getByTestId('switch-input-cmp')
    expect(el).toBeInTheDocument()

    fireEvent.click(el)

    expect(onClick).toHaveBeenCalled()
    expect(onClick).toHaveBeenCalledWith(!val)
  })
})
