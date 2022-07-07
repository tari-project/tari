import Loading from '../Loading'
import Text from '../Text'

import {
  ButtonContentWrapper,
  IconWrapper,
  LoadingIconWrapper,
  StyledButton,
  StyledButtonText,
  StyledLink,
  StyledLinkLikeButton,
} from './styles'
import { ButtonProps } from './types'

/**
 * Button component
 *
 * @param {ReactNode | string} children - the button content. String is wrapped with the <Text /> component.
 * @param {ButtonVariantType} [variant='primary'] - ie. 'primary', 'secondary', 'button-in-text'
 * @param {CSSProperties} [style] - the style applied to the outter element.
 * @param {ButtonType} [type='button'] - the HTML button type, ie. 'submit'
 * @param {ButtonSizeType} [size='medium'] - the size of the button
 * @param {string} [href] - if applied, it renders <a /> element with a given href
 * @param {ReactNode} [leftIcon] - element rendered on left side of the button
 * @param {string} [leftIconColor] - custom icon color
 * @param {ReactNode} [rightIcon] - element rendered on right side of the button
 * @param {boolean} [autosizeIcons='true'] - by default, it resizes any svg element set as leftIcon or rightIcon to a given dimensions (16x16px)
 * @param {boolean} [loading] - displays the loader
 * @param {boolean} [fullWidth] - render with width = 100%
 * @param {() => void} [onClick] - on button click
 * @param {string} [testId] - react test id
 *
 * @example
 * <Button
 *   type='submit'
 *   variant='secondary'
 *   size='small'
 *   rightIcon={<SvgSetting />}
 *   leftIcon={<SvgSetting />}
 * >
 *  String or {ReactNode}
 * </Button>
 */

const Button = ({
  children,
  disabled,
  style,
  variant,
  type = 'button',
  size = 'medium',
  href,
  leftIcon,
  leftIconColor,
  rightIcon,
  autosizeIcons = true,
  onClick,
  loading,
  fullWidth,
  testId = 'button-cmp',
}: ButtonProps) => {
  let btnText = children

  if (typeof children === 'string') {
    btnText = (
      <StyledButtonText size={size}>
        <Text
          as='span'
          type={size === 'small' ? 'smallMedium' : 'defaultMedium'}
          testId='button-text-wrapper'
        >
          {children}
        </Text>
      </StyledButtonText>
    )
  }

  const btnContent = (
    <>
      {leftIcon ? (
        <IconWrapper
          $spacing={'right'}
          $autosizeIcon={autosizeIcons}
          $variant={variant}
          $disabled={disabled}
          $leftIconColor={leftIconColor}
          data-testid='button-left-icon'
        >
          {leftIcon}
        </IconWrapper>
      ) : null}
      <ButtonContentWrapper disabled={disabled}>{btnText}</ButtonContentWrapper>
      {rightIcon ? (
        <IconWrapper
          $spacing={'left'}
          $autosizeIcon={autosizeIcons}
          $variant={variant}
          $disabled={disabled}
          data-testid='button-right-icon'
        >
          {rightIcon}
        </IconWrapper>
      ) : null}
      {loading ? (
        <LoadingIconWrapper data-testid='button-loading-icon'>
          <Loading loading size='14px' />
        </LoadingIconWrapper>
      ) : null}
    </>
  )

  if (variant === 'button-in-text') {
    return (
      <StyledLink
        as='button'
        onClick={onClick}
        style={style}
        variant='text'
        data-testid={testId}
        disabled={disabled}
      >
        {btnContent}
      </StyledLink>
    )
  }

  if (type === 'link' || href) {
    if (variant && variant !== 'text') {
      return (
        <StyledLinkLikeButton
          href={href}
          onClick={onClick}
          style={style}
          target='_blank'
          variant={variant}
          data-testid={testId}
          $fullWidth={fullWidth}
        >
          {btnContent}
        </StyledLinkLikeButton>
      )
    }

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
      $fullWidth={fullWidth}
    >
      {btnContent}
    </StyledButton>
  )
}

export default Button
