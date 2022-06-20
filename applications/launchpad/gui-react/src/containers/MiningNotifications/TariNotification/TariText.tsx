import { CSSProperties } from 'react'
import { useTheme } from 'styled-components'

import Text from '../../../components/Text'
import { TextType } from '../../../components/Text/types'

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

  const parts = children.split('tari')
  const textElements = parts.flatMap((textPart, index) =>
    index === parts.length - 1
      ? [
          <Text key={textPart} as='span' type={type}>
            {textPart}
          </Text>,
        ]
      : [
          <Text key={textPart} as='span' type={type}>
            {textPart}
          </Text>,
          <Text
            key={`tari-${index}`}
            as='span'
            type={type}
            color={theme.accent}
          >
            tari
          </Text>,
        ],
  )

  return <span style={style}>{textElements}</span>
}

export default TariText
