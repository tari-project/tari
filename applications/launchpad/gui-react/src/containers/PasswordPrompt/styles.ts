import styled from 'styled-components'

import SvgTariSignet from '../../styles/Icons/TariSignet'

export const TariBackgroundSignet = styled(SvgTariSignet)`
  color: ${({ theme }) => theme.backgroundImage};
  height: 80px;
  width: 80px;
  position: absolute;
  z-index: 0;
  pointer-events: none;
  right: ${({ theme }) => theme.spacing()};
  top: ${({ theme }) => theme.spacing()};
`
