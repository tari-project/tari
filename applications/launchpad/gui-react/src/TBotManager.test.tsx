import { render, screen, cleanup } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'
import { Provider } from 'react-redux'
import themes from './styles/themes'
import TBotManager from './TBotManager'
import { store } from './store'

afterEach(cleanup)

describe('TBot', () => {
  it('should render the TBotManager component without crashing when set to open', () => {
    render(
      <Provider store={store}>
        <ThemeProvider theme={themes.light}>
          <TBotManager messages={['test']} />
        </ThemeProvider>
        ,
      </Provider>,
    )

    const el = screen.getByText('test')
    expect(el).toBeInTheDocument()
  })
})
