import { useSelector } from 'react-redux'
import styled, { ThemeProvider } from 'styled-components'

import logo from './logo.svg'
import './App.css'
import { ThemeProvider } from 'styled-components'
import GlobalStyle from './styles/globalStyles'

import HomePage from './pages/home'

import './styles/App.css'

const AppContainer = styled.div`
  background: ${({ theme }) => theme.background};
  display: flex;
  flex: 1;
  overflow: hidden;
  border-radius: 10;
`

const App = () => {
  const themeConfig = useSelector(selectThemeConfig)

  return (
    <ThemeProvider theme={themeConfig}>
      <AppContainer>
        <HomePage />
      </AppContainer>
    </ThemeProvider>
  )
}

export default App
