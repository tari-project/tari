import { useEffect } from 'react'
import { invoke } from '@tauri-apps/api/tauri'
import { listen } from '@tauri-apps/api/event'
import styled, { ThemeProvider } from 'styled-components'

import { selectThemeConfig } from './store/app/selectors'
import { actions } from './store/services'
import { Service } from './store/services/types'
import { useAppSelector, useAppDispatch } from './store/hooks'

import HomePage from './pages/home'
import { loadDefaultServiceSettings } from './store/settings/thunks'
import './styles/App.css'

const AppContainer = styled.div`
  background: ${({ theme }) => theme.background};
  display: flex;
  flex: 1;
  overflow: hidden;
  border-radius: 10;
`
const App = () => {
  const themeConfig = useAppSelector(selectThemeConfig)
  const dispatch = useAppDispatch()
  dispatch(loadDefaultServiceSettings())

  useEffect(() => {
    invoke('events')
  }, [])

  useEffect(() => {
    let unsubscribe

    const listenToSystemEvents = async () => {
      unsubscribe = await listen('tari://docker-system-event', event => {
        console.log('System event: ', event.payload)
      })
    }

    // listenToSystemEvents()

    return unsubscribe
  }, [])

  const launch = async (service: Service) => {
    dispatch(actions.start(service))
  }

  const magic = async () => {
    await launch(Service.Tor)
    await launch(Service.BaseNode)
  }

  return (
    <ThemeProvider theme={themeConfig}>
      <AppContainer>
        <div
          style={{
            display: 'flex',
            flexDirection: 'column',
            alignItems: 'center',
            justifyContent: 'center',
          }}
        >
          <button onClick={() => launch(Service.Tor)}>tor</button>
          <button onClick={() => launch(Service.BaseNode)}>base_node</button>
          <button onClick={magic}>magic</button>
        </div>
        <HomePage />
      </AppContainer>
    </ThemeProvider>
  )
}

export default App
