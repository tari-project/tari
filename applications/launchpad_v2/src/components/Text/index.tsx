import { StyledText } from './styles'
import { TextProps } from './types'
import styles from '../../styles/styles'

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

const Text = ({ type = 'defaultMedium', color, children }: TextProps) => {
  const textStyles = {
    color: color,
    ...styles.typography[type],
  }

  return <StyledText style={textStyles}>{children}</StyledText>
}

export default Text
