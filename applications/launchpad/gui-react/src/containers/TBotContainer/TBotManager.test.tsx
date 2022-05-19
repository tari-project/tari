import { render, screen, cleanup } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'
import { Provider } from 'react-redux'
import themes from '../../styles/themes'
import { store } from '../../store'
import TBotManager from './TBotManager'

afterEach(cleanup)

describe('TBot', () => {
  it('should render the TBotManager component without crashing when set to open', () => {
    render(
      <Provider store={store}>
        <ThemeProvider theme={themes.light}>
          <TBotManager messages={['testText']} />
        </ThemeProvider>
        ,
      </Provider>,
    )

    const el = screen.getByText('testText')
    expect(el).toBeInTheDocument()
  })
})
