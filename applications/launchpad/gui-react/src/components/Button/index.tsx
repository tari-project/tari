import Loading from '../Loading'

import {
  ButtonText,
  IconWrapper,
  LoadingIconWrapper,
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
  loading,
  testId = 'button-cmp',
}: ButtonProps) => {
  const btnContent = (
    <>
      {leftIcon ? <IconWrapper>{leftIcon}</IconWrapper> : null}
      <ButtonText>{children}</ButtonText>
      {rightIcon ? <IconWrapper>{rightIcon}</IconWrapper> : null}
      {loading ? (
        <LoadingIconWrapper>
          <Loading loading size='1em' />
        </LoadingIconWrapper>
      ) : null}
    </>
  )

  if (type === 'button-in-text') {
    return (
      <StyledLink
        as='button'
        onClick={onClick}
        style={style}
        variant='text'
        data-testid={testId}
      >
        {btnContent}
      </StyledLink>
    )
  }

  if (type === 'link' || href) {
    return (
      <StyledLink
        href={href}
        onClick={onClick}
        style={style}
        target='_blank'
        variant='text'
        data-testid={testId}
      >
        {btnContent}
      </StyledLink>
    )
  }

  return (
    <StyledButton
      disabled={disabled}
      type={type}
      onClick={onClick}
      style={style}
      variant={variant}
      data-testid={testId}
    >
      {btnContent}
    </StyledButton>
  )
}

export default Button
