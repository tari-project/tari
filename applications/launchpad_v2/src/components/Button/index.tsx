import { useContext } from 'react'
import { CSSProperties, ThemeContext } from 'styled-components'

import { ButtonText, IconWrapper, StyledButton, StyledLink } from './styles'
import { ButtonProps } from './types'

const Button = ({
  children,
  style,
  variant,
  type = 'button',
  href,
  leftIcon,
  rightIcon,
  onClick,
}: ButtonProps) => {
  const theme = useContext(ThemeContext)

  let baseStyle: CSSProperties = {}

  switch (variant) {
    case 'text':
      baseStyle = {
        background: 'transparent',
        color: theme.secondary,
      }
      break
    default:
      baseStyle = {
        background: theme.tariGradient,
        color: theme.primary,
      }
      break
  }

  if (style) {
    baseStyle = { ...baseStyle, ...style }
  }

  const btnContent = (
    <>
      {leftIcon ? <IconWrapper>{leftIcon}</IconWrapper> : null}
      <ButtonText>{children}</ButtonText>
      {rightIcon ? <IconWrapper>{rightIcon}</IconWrapper> : null}
    </>
  )

  if (type === 'link' || href) {
    return (
      <StyledLink
        href={href}
        onClick={() => onClick && onClick()}
        style={baseStyle}
      >
        {btnContent}
      </StyledLink>
    )
  }

  return (
    <StyledButton
      type={type}
      onClick={() => onClick && onClick()}
      style={baseStyle}
    >
      {btnContent}
    </StyledButton>
  )
}

export default Button
