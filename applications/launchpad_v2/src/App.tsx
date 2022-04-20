import { useSelector } from 'react-redux'
import styled, { ThemeProvider } from 'styled-components'

import logo from './logo.svg'
import './App.css'
import { ThemeProvider } from 'styled-components'
import GlobalStyle from './styles/globalStyles'

import HomePage from './pages/home'

import './styles/App.css'

  return (
    <ThemeProvider theme={{}}>
      <GlobalStyle />
      <>
        <div className="App">
          <header className="App-header">
            <img src={logo} className="App-logo" alt="logo" />
            <p>
          Edit <code>src/App.tsx</code> and save to reload.
            </p>
            <a
              className="App-link"
              href="https://reactjs.org"
              target="_blank"
              rel="noopener noreferrer"
            >
          Learn React
            </a>
            <p>available docker images:<br/>
              {images.map(img => <em key={img}>{img}{', '}</em>)}
            </p>
          </header>
        </div>
      </>
    </ThemeProvider>
  )
}

export default App
