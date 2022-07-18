import { useEffect, useState, useMemo } from 'react'
import styled, { ThemeProvider } from 'styled-components'
import { useHotkeys } from 'react-hotkeys-hook'
import 'uplot/dist/uPlot.min.css'

import { useAppSelector, useAppDispatch } from './store/hooks'
import useTransactionsRepository from './persistence/transactionsRepository'
import getStatsRepository from './persistence/statsRepository'
import { init } from './store/app'
import {
  selectOnboardingComplete,
  selectThemeConfig,
} from './store/app/selectors'
import { useSystemEvents } from './useSystemEvents'
import { useWalletEvents } from './useWalletEvents'
import { useDockerEvents } from './useDockerEvents'
import { useCheckDockerImages } from './useCheckDockerImages'
import { useDockerImageDownloadListener } from './hooks/useDockerImageDownloadListener'
import HomePage from './pages/home'
import './styles/App.css'

import useMiningScheduling from './useMiningScheduling'
import TBotContainer from './containers/TBotContainer'
import MiningNotifications from './containers/MiningNotifications'
import Onboarding from './pages/onboarding'
import PasswordPrompt from './containers/PasswordPrompt'
import { hideSplashscreen } from './splashscreen'
import { openTerminalCmd } from './commands'
import { useDockerTBotQueue } from './useDockerTBotQueue'
import { useInternetCheck } from './useInternetCheck'
import { useWaitingWalletPassConfirm } from './useWaitingWalletPassConfirm'

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
  const statsRepository = useMemo(getStatsRepository, [])

  useWalletEvents({ dispatch, transactionsRepository })
  useMiningScheduling()
  useCheckDockerImages({ dispatch })
  useDockerTBotQueue({ dispatch })
  useWaitingWalletPassConfirm({ dispatch })

  useEffect(() => {
    statsRepository.removeOld()
  }, [])

  return children
}

const App = () => {
  const dispatch = useAppDispatch()
  const themeConfig = useAppSelector(selectThemeConfig)
  const onboardingComplete = useAppSelector(selectOnboardingComplete)

  const [initialized, setInitialized] = useState(false)

  useHotkeys('ctrl+t,cmd+t', () => {
    openTerminalCmd()
  })

  useEffect(() => {
    const callInitActionInStore = async () => {
      try {
        await dispatch(init()).unwrap()
        setInitialized(true)
        hideSplashscreen()
      } catch (err) {
        // eslint-disable-next-line no-console
        console.error('App exception:', err)
        throw err
      }
    }

    callInitActionInStore()
  }, [])

  useSystemEvents({ dispatch })
  useDockerEvents({ dispatch })
  useDockerImageDownloadListener({ dispatch })
  useInternetCheck({ dispatch })

  return (
    <ThemeProvider theme={themeConfig}>
      <AppContainer>
        {!onboardingComplete ? (
          initialized ? (
            <Onboarding />
          ) : null
        ) : initialized ? (
          <PasswordPrompt>
            <OnboardedAppContainer>
              <HomePage />
              <TBotContainer />
              <MiningNotifications />
            </OnboardedAppContainer>
          </PasswordPrompt>
        ) : null}
      </AppContainer>
    </ThemeProvider>
  )
}

export default App
