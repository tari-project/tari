import { cleanup, render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'
import themes from '../../styles/themes'

import TabContent from '../TabContent'

afterEach(cleanup)

describe('TabContent', () => {
  it('should render the component without crashing', () => {
    const testText = 'testing'
    render(
      <ThemeProvider theme={themes.light}>
        <TabContent text={testText} />
      </ThemeProvider>,
    )

    expect(screen.getByText(testText)).toBeInTheDocument()
  })
})
