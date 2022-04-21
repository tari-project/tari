import { StyledBox } from './styles'

import { BoxProps } from './types'

const Box = ({ children, gradient, border, style: inlineStyle }: BoxProps) => {
  const style = {
    border: border === false ? 'none' : undefined,
    background:
      gradient &&
      `
      linear-gradient(
      45deg,
      ${gradient.start} 0%,
      ${gradient.end} 100%
    )`,
    ...inlineStyle,
  }

  return <StyledBox style={style}>{children}</StyledBox>
}

export default Box
