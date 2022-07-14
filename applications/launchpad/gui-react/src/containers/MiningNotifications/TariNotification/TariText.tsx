import { CSSProperties } from 'react'
import { useTheme } from 'styled-components'

import Text from '../../../components/Text'
import { TextType } from '../../../components/Text/types'

/**
 * @name TariText
 * @description Text component that renders text with all `tari` and `Tari` instances in accent colour
 *
 * @prop {string} children - text to render with accent
 * @prop {CSSProperties} [style] - optional override for main text container
 * @prop {TextType} [type] - type of Text to render
 */
const TariText = ({
  children,
  style,
  type = 'subheader',
}: {
  children: string
  style?: CSSProperties
  type?: TextType
}) => {
  const theme = useTheme()

  const parts = children
    .replaceAll('tari', '_tari_')
    .replaceAll('Tari', '_Tari_')
    .split('_')
  const textElements = parts.map((part, index) => (
    <Text
      key={`${part}-${index}`}
      as='span'
      color={part.toLowerCase() === 'tari' ? theme.accent : theme.primary}
      type={type}
    >
      {part}
    </Text>
  ))

  return <span style={style}>{textElements}</span>
}

export default TariText
