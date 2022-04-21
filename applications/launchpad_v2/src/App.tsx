import { useSelector } from 'react-redux'
import styled, { ThemeProvider } from 'styled-components'

import { selectThemeConfig } from './store/app/selectors'

import HomePage from './pages/home'

import './styles/App.css'
import GlobalStyle from './styles/globalStyles'

const AppContainer = styled.div`
  background: ${({ theme }) => theme.background};
  display: flex;
  flex: 1;
  overflow: hidden;
  borderradius: 10;
`

const App = () => {
  const themeConfig = useSelector(selectThemeConfig)

  return (
    <ThemeProvider theme={themeConfig}>
      <GlobalStyle />
      <AppContainer>
        <HomePage />
      </AppContainer>
    </ThemeProvider>
  )
}

export default App
