import 'react-devtools' // @TODO: remove this import before final Production deployment!!!
import styled, { ThemeProvider } from 'styled-components'

import { useAppSelector, useAppDispatch } from './store/hooks'
import useTransactionsRepository from './persistence/transactionsRepository'
import {
  selectOnboardingComplete,
  selectThemeConfig,
} from './store/app/selectors'

import { useSystemEvents } from './useSystemEvents'
import { useWalletEvents } from './useWalletEvents'
import HomePage from './pages/home'
import { loadDefaultServiceSettings } from './store/settings/thunks'
import './styles/App.css'

import useMiningSimulator from './useMiningSimulator'
import useMiningScheduling from './useMiningScheduling'
import TBotContainer from './containers/TBotContainer'
import MiningNotifications from './containers/MiningNotifications'
import Onboarding from './pages/onboarding'

const AppContainer = styled.div`
  background: ${({ theme }) => theme.background};
  display: flex;
  flex: 1;
  overflow: hidden;
  border-radius: 10;
`

const App = () => {
  const transactionsRepository = useTransactionsRepository()
  const themeConfig = useAppSelector(selectThemeConfig)
  const dispatch = useAppDispatch()
  const onboardingComplete = useAppSelector(selectOnboardingComplete)

  dispatch(loadDefaultServiceSettings())

  useSystemEvents({ dispatch })

  useWalletEvents({ transactionsRepository })

  useMiningSimulator()

  useMiningScheduling()

  return (
    <ThemeProvider theme={themeConfig}>
      <AppContainer>
        {!onboardingComplete ? (
          <Onboarding />
        ) : (
          <>
            <HomePage />
            <TBotContainer />
            <MiningNotifications />
          </>
        )}
      </AppContainer>
    </ThemeProvider>
  )
}

export default App
