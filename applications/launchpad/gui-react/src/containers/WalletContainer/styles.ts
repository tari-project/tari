import styled from 'styled-components'

import SvgTariSignet from '../../styles/Icons/TariSignet'

export const TariBackgroundSignet = styled(SvgTariSignet)`
  color: ${({ theme }) => theme.backgroundImage};
  height: 80px;
  width: 80px;
  position: absolute;
  right: ${({ theme }) => theme.spacing()};
  top: ${({ theme }) => theme.spacing()};
`

export const TariSignet = styled(SvgTariSignet)``

export const TariAmountContainer = styled.div`
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-top: ${({ theme }) => theme.spacingVertical()};
  margin-bottom: ${({ theme }) => theme.spacingVertical(2)};
`
