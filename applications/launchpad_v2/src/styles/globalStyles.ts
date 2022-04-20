import { createGlobalStyle } from 'styled-components'

import { AvenirRegular, AvenirMedium, AvenirHeavy } from '../assets/fonts/fonts'

/**
 * Global styles
 */

const GlobalStyle = createGlobalStyle`
  @font-face {
    src: url(${AvenirRegular});
    font-family: 'AvenirRegular'
  }
  @font-face {
    src: url(${AvenirMedium});
    font-family: 'AvenirMedium'
  }
  @font-face {
    src: url(${AvenirHeavy});
    font-family: 'AvenirHeavy'
  }
`

export default GlobalStyle