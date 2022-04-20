import { createGlobalStyle } from 'styled-components'

import { AvenirRegular, AvenirMedium, AvenirHeavy } from '../../assets/fonts/fonts'

import { TextProps } from './types'

import styles from '../../styles/styles'

/**
 * Global style rule to make fonts files accessible
 */
const GlobalFonts = createGlobalStyle`
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
/**
 * @name Text
 * 
 * @typedef TextProps
 * @prop {'header' | 'subheader' | 'defaultHeavy' | 'defaultMedium' | 'defaultUnder' | 'smallHeavy' | 'smallMedium' | 'smallUnder' | 'microHeavy' | 'microRegular'  | 'microOblique' } type - text styles
 * @prop {ReactNode} children - text content to display
 * @prop {string} [color] - font color 
 * 
 * @example
 * <Text type='defaultMedium' color={styles.colors.dark.primary}>...text goes here...</Text>
 */

const Text = ({
  type,
  color,
  children,
}: TextProps) => {
  const textStyles = {
    color: color,
    ...styles.typography[type]
  }
  return (
    <>
      <GlobalFonts />
      <p style={textStyles}>{children}</p>
    </>
  )
}

export default Text
