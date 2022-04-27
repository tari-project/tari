import {
  DisabledButton,
  ButtonText,
  IconWrapper,
  StyledButton,
  StyledLink,
} from './styles'
import { ButtonProps } from './types'

const Button = ({
  children,
  disabled,
  style,
  variant,
  type = 'button',
  href,
  leftIcon,
  rightIcon,
  onClick,
}: ButtonProps) => {
  const btnContent = (
    <>
      {leftIcon ? <IconWrapper>{leftIcon}</IconWrapper> : null}
      <ButtonText>{children}</ButtonText>
      {rightIcon ? <IconWrapper>{rightIcon}</IconWrapper> : null}
    </>
  )

  if (type === 'link' || href) {
    return (
      <StyledLink href={href} onClick={onClick} style={style} variant={variant}>
        {btnContent}
      </StyledLink>
    )
  }

  if (variant === 'disabled') {
    return (
      <DisabledButton
        disabled={disabled}
        type={type}
        onClick={onClick}
        style={style}
        variant={variant}
      >
        {btnContent}
      </DisabledButton>
    )
  }

  return (
    <StyledButton
      disabled={disabled}
      type={type}
      onClick={onClick}
      style={style}
      variant={variant}
    >
      {btnContent}
    </StyledButton>
  )
}

export default Button
