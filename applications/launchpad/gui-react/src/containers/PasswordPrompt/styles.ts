import styled from 'styled-components'

import SvgTariSignet from '../../styles/Icons/TariSignet'
import SvgMoneroSignet from '../../styles/Icons/MoneroSignet'

export const SignetsContainer = styled.div`
  position: absolute;
  z-index: 0;
  pointer-events: none;
  right: ${({ theme }) => theme.spacing()};
  top: ${({ theme }) => theme.spacing()};
  display: flex;
  flex-direction: column;
  row-gap: ${({ theme }) => theme.spacing(0.5)};
`

export const TariBackgroundSignet = styled(SvgTariSignet)`
  color: ${({ theme }) => theme.disabledPrimaryButton};
  height: 80px;
  width: 80px;
`

export const MoneroBackgroundSignet = styled(SvgMoneroSignet)`
  color: ${({ theme }) => theme.disabledPrimaryButton};
  height: 80px;
  width: 80px;
`
