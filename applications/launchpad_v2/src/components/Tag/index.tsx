import { useContext } from 'react'
import { ThemeContext, CSSProperties } from 'styled-components'

import Text from '../Text'

import { TagProps } from './types'

import { TagContainer, IconWrapper } from './styles'

/**
 * @name Tag
 * @typedef TagProps
 *
 * @prop {ReactNode} [children] - text content to display
 * @prop {CSSProperties} [style] - optional component styles
 * @prop {'blue' | 'running' | 'warning' | 'expert'} [type] - tag types to determine color settings
 * @prop {ReactNode} [icon] - optional SVG icon
 * @prop {ReactNode} [subText] - optional additional tag text
 *
 * @example
 * <Tag type='running' style={extraStyles} icon={<someIconComponent/>} subText='Mainnet'>
 *    Running
 * </Tag
 */

const Tag = ({
  children,
  style,
  type,
  variant = 'small',
  icon,
  subText,
}: TagProps) => {
  const theme = useContext(ThemeContext)

  let baseStyle: CSSProperties = {}
  let textStyle: CSSProperties = {}

  switch (type) {
    case 'blue':
      baseStyle = {
        backgroundColor: theme.info,
        color: theme.infoText,
      }
      break
    case 'running':
      baseStyle = {
        backgroundColor: theme.on,
        color: theme.onText,
      }
      break
    case 'warning':
      baseStyle = {
        backgroundColor: theme.warning,
        color: theme.warningText,
      }
      break
    case 'expert':
      baseStyle = {
        backgroundColor: theme.expert,
      }
      textStyle = {
        backgroundImage: theme.expertText,
        WebkitBackgroundClip: 'text',
        color: 'transparent',
      }
  }

  if (style) {
    baseStyle = { ...baseStyle, ...style }
  }

  const tagContent = (
    <>
      {icon && <IconWrapper style={baseStyle}>{icon}</IconWrapper>}

      <Text
        type={variant === 'large' ? 'smallHeavy' : 'microMedium'}
        style={textStyle}
      >
        {children}
      </Text>

      {subText && (
        <Text
          style={{ marginLeft: '4px' }}
          type='microMedium'
          color={theme.onTextLight}
        >
          {subText}
        </Text>
      )}
    </>
  )
  return (
    <TagContainer style={baseStyle} variant={variant}>
      {tagContent}
    </TagContainer>
  )
}

export default Tag
