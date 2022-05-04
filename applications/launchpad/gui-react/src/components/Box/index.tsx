import { StyledBox } from './styles'

import { BoxProps } from './types'

/**
 * A box with standardized border radius, padding etc.
 *
 * @prop {ReactNode} children - elements to render inside the box
 * @prop {Gradient} gradient - optional gradient definition for box background
 * @prop {boolean} border - whether to show box border or not
 * @prop {CSSProperties} style - prop allowing to override all styles of the box
 *
 * @typedef Gradient
 * @prop {string} start - color of gradient start
 * @prop {string} end - color on gradient end
 * @prop {number} rotation - gradient rotation in degress (45 by default)
 */
const Box = ({
  children,
  gradient,
  border,
  style: inlineStyle,
  testId = 'box-cmp',
}: BoxProps) => {
  const style = {
    border: border === false ? 'none' : undefined,
    background:
      gradient &&
      `
      linear-gradient(
      ${gradient.rotation || 45}deg,
      ${gradient.start} 0%,
      ${gradient.end} 100%
    )`,
    ...inlineStyle,
  }

  return (
    <StyledBox style={style} data-testid={testId}>
      {children}
    </StyledBox>
  )
}

export default Box
