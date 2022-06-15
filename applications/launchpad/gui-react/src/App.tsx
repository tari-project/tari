import { useState } from 'react'
import 'react-devtools' // @TODO: remove this import before final Production deployment!!!
import styled, { ThemeProvider } from 'styled-components'

import { useAppSelector, useAppDispatch } from './store/hooks'
import { selectThemeConfig } from './store/app/selectors'
import getTransactionsRepository from './persistence/minedTransactionsRepository'
import { useSystemEvents } from './useSystemEvents'
import { useWalletEvents } from './useWalletEvents'
import HomePage from './pages/home'
import { loadDefaultServiceSettings } from './store/settings/thunks'
import './styles/App.css'

import useMiningSimulator from './useMiningSimulator'
import useMiningScheduling from './useMiningScheduling'
import TBotContainer from './containers/TBotContainer'
import Onboarding from './pages/onboarding'

const AppContainer = styled.div`
  background: ${({ theme }) => theme.background};
  display: flex;
  flex: 1;
  overflow: hidden;
  border-radius: 10;
`

const transactionsRepository = getTransactionsRepository()
const App = () => {
  const themeConfig = useAppSelector(selectThemeConfig)
  const dispatch = useAppDispatch()

  const [onboarding, setOnboarding] = useState(false)

  dispatch(loadDefaultServiceSettings())

  useSystemEvents({ dispatch })

  useWalletEvents({ transactionsRepository })

  useMiningSimulator()

  useMiningScheduling()

  return (
    <ThemeProvider theme={themeConfig}>
      <AppContainer>
        {onboarding ? (
          <Onboarding close={() => setOnboarding(false)} />
        ) : (
          <>
            <HomePage />
            <TBotContainer />
          </>
        )}
      </AppContainer>
    </ThemeProvider>
  )
}

export default App
