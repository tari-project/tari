import { CSSProperties, useTheme } from 'styled-components'

import Text from '../Text'

import { TagProps } from './types'

import { TagContainer, IconWrapper } from './styles'

/**
 * Tag component
 *
 * @prop {ReactNode} [children] - text content to display
 * @prop {CSSProperties} [style] - optional component styles
 * @prop {'info' | 'running' | 'warning' | 'expert' | 'light'} [type] - tag types to determine color settings
 * @prop {boolean} [expertSec] - specific usage of expert tag type
 * @prop {ReactNode} [icon] - optional SVG icon
 * @prop {ReactNode} [subText] - optional additional tag text
 * @prop {boolean} [inverted] - optional prop indicating whether tag should be rendered in inverted coloring
 * @prop {boolean} [dark] - special style case, e.g. dashboard running
 * @prop {boolean} [darkAlt] - special style case, e.g. base node running
 * @prop {boolean} [expertSec] - special style case for expert tag type
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
  dark,
  darkAlt,
  expertSec,
}: TagProps) => {
  const theme = useTheme()

  let baseStyle: CSSProperties = {}
  let textStyle: CSSProperties = {}

  let runningTagBackgroundColor
  let runningTagTextColor

  if (dark) {
    runningTagBackgroundColor = theme.dashboardRunningTagBackground
    runningTagTextColor = theme.dashboardRunningTagText
  } else if (darkAlt) {
    runningTagBackgroundColor = theme.baseNodeRunningTagBackground
    runningTagTextColor = theme.baseNodeRunningTagText
  } else {
    runningTagBackgroundColor = theme.runningTagBackground
    runningTagTextColor = theme.runningTagText
  }

  switch (type) {
    case 'running':
      baseStyle = {
        backgroundColor: inverted
          ? theme.transparent(theme.onText, 40)
          : runningTagBackgroundColor,
      }
      textStyle = {
        color: inverted ? theme.onTextLight : runningTagTextColor,
      }
      break
    case 'warning':
      baseStyle = {
        backgroundColor: theme.warningTag,
      }
      textStyle = {
        color: theme.warningText,
      }
      break
    case 'expert':
      baseStyle = {
        backgroundColor: theme.expert,
      }
      if (expertSec) {
        textStyle = {
          color: theme.expertSecText,
        }
      } else {
        textStyle = {
          backgroundImage: theme.expertText,
          WebkitBackgroundClip: 'text',
          color: 'transparent',
        }
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
        backgroundColor: theme.infoTag,
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
          style={{ marginLeft: '6px' }}
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
