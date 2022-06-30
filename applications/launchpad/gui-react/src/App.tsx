import { useEffect, useState } from 'react'
import styled, { ThemeProvider } from 'styled-components'

import { useAppSelector, useAppDispatch } from './store/hooks'
import useTransactionsRepository from './persistence/transactionsRepository'
import { init } from './store/app'
import {
  selectOnboardingComplete,
  selectThemeConfig,
} from './store/app/selectors'
import { useSystemEvents } from './useSystemEvents'
import { useWalletEvents } from './useWalletEvents'
import { useDockerEvents } from './useDockerEvents'
import HomePage from './pages/home'
import './styles/App.css'

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
const OnboardedAppContainer = ({
  children,
}: {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  children: any
}) => {
  const transactionsRepository = useTransactionsRepository()
  const dispatch = useAppDispatch()

  useSystemEvents({ dispatch })
  useWalletEvents({ dispatch, transactionsRepository })
  useMiningScheduling()

  return children
}

const App = () => {
  const dispatch = useAppDispatch()
  const themeConfig = useAppSelector(selectThemeConfig)
  const onboardingComplete = useAppSelector(selectOnboardingComplete)

  const [initialized, setInitialized] = useState(false)

  useEffect(() => {
    const callInitActionInStore = async () => {
      await dispatch(init()).unwrap()
      setInitialized(true)
    }

    callInitActionInStore()
  }, [])

  useSystemEvents({ dispatch })
  useDockerEvents({ dispatch })

  return (
    <ThemeProvider theme={themeConfig}>
      <AppContainer>
        {!onboardingComplete ? (
          initialized ? (
            <Onboarding />
          ) : null
        ) : initialized ? (
          <OnboardedAppContainer>
            <HomePage />
            <TBotContainer />
            <MiningNotifications />
          </OnboardedAppContainer>
        ) : null}
      </AppContainer>
    </ThemeProvider>
  )
}

export default App
