import { AnchorHTMLAttributes } from 'react'

import { StyledAnchor } from './styles'

const Link = (props: AnchorHTMLAttributes<HTMLAnchorElement>) => (
  <StyledAnchor target='_blank' {...props} />
)

export default Link
