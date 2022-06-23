import 'react-devtools' // @TODO: remove this import before final Production deployment!!!
import { useEffect, useState } from 'react'
import styled, { ThemeProvider } from 'styled-components'

import { useAppSelector, useAppDispatch } from './store/hooks'
import useTransactionsRepository from './persistence/transactionsRepository'
import { actions as dockerImagesActions } from './store/dockerImages'
import {
  selectOnboardingComplete,
  selectThemeConfig,
} from './store/app/selectors'
import { useSystemEvents } from './useSystemEvents'
import { useWalletEvents } from './useWalletEvents'
import { useDockerEvents } from './useDockerEvents'
import HomePage from './pages/home'
import { loadDefaultServiceSettings } from './store/settings/thunks'
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
// eslint-disable-next-line @typescript-eslint/no-explicit-any
const OnboardedAppContainer = ({ children }: { children: any }) => {
  const [initialized, setInitialized] = useState(false)
  const transactionsRepository = useTransactionsRepository()
  const dispatch = useAppDispatch()

  useEffect(() => {
    const init = async () => {
      await dispatch(loadDefaultServiceSettings()).unwrap()
      await dispatch(dockerImagesActions.getDockerImageList()).unwrap()
      setInitialized(true)
    }

    init()
  }, [])

  useSystemEvents({ dispatch })
  useWalletEvents({ dispatch, transactionsRepository })
  useDockerEvents({ dispatch })
  useMiningScheduling()

  if (!initialized) {
    return null
  }

  return children
}

const App = () => {
  const themeConfig = useAppSelector(selectThemeConfig)
  const onboardingComplete = useAppSelector(selectOnboardingComplete)

  return (
    <ThemeProvider theme={themeConfig}>
      <AppContainer>
        {!onboardingComplete ? (
          <Onboarding />
        ) : (
          <OnboardedAppContainer>
            <HomePage />
            <TBotContainer />
            <MiningNotifications />
          </OnboardedAppContainer>
        )}
      </AppContainer>
    </ThemeProvider>
  )
}

export default App
