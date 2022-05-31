import { render, screen, cleanup } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'
import { Provider } from 'react-redux'
import { configureStore } from '@reduxjs/toolkit'
import themes from '../../styles/themes'
import { rootReducer } from '../../store'
import TBotContainer from './'

afterEach(cleanup)

/**
 * @TODO - update this test
 */
describe('TBot', () => {
  it('should render the TBotManager component without crashing when set to open', () => {
    render(
      <Provider
        store={configureStore({
          reducer: rootReducer,
          preloadedState: {
            tbot: {
              messageQueue: ['testText'],
              open: true,
            },
          },
        })}
      >
        <ThemeProvider theme={themes.light}>
          <TBotContainer />
        </ThemeProvider>
        ,
      </Provider>,
    )
    const el = screen.getByText('testText')
    expect(el).toBeInTheDocument()
  })
})
