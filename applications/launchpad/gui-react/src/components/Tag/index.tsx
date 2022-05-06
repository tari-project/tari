import { CSSProperties, useTheme } from 'styled-components'

import Text from '../Text'

import { TagProps } from './types'

import { TagContainer, IconWrapper } from './styles'

/**
 * @name Tag
 * @typedef TagProps
 *
 * @prop {ReactNode} [children] - text content to display
 * @prop {CSSProperties} [style] - optional component styles
 * @prop {'info' | 'running' | 'warning' | 'expert' | 'light'} [type] - tag types to determine color settings
 * @prop {ReactNode} [icon] - optional SVG icon
 * @prop {ReactNode} [subText] - optional additional tag text
 * @prop {boolean} [inverted] - optional prop indicating whether tag should be rendered in inverted coloring
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
  inverted,
}: TagProps) => {
  const theme = useTheme()

  let baseStyle: CSSProperties = {}
  let textStyle: CSSProperties = {}

  switch (type) {
    case 'running':
      baseStyle = {
        backgroundColor: inverted
          ? theme.transparent(theme.onText, 40)
          : theme.on,
      }
      textStyle = {
        color: inverted ? theme.inverted.accentSecondary : theme.onText,
      }
      break
    case 'warning':
      baseStyle = {
        backgroundColor: theme.warning,
      }
      textStyle = {
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
      break
    case 'light':
      baseStyle = {
        backgroundColor: theme.lightTag,
      }
      textStyle = {
        color: theme.lightTagText,
      }
      break
    // info tag type is default
    default:
      baseStyle = {
        backgroundColor: theme.info,
      }
      textStyle = {
        color: theme.infoText,
      }
      break
  }

  if (style) {
    baseStyle = { ...baseStyle, ...style }
  }

  const tagContent = (
    <>
      {icon && (
        <IconWrapper type={type} textStyle={textStyle}>
          {icon}
        </IconWrapper>
      )}

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
    <TagContainer style={baseStyle} data-testid='tag-component'>
      {tagContent}
    </TagContainer>
  )
}

export default Tag
