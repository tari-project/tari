import styled from 'styled-components'

import SvgTariSignet from '../../styles/Icons/TariSignet'
import type { ExpertViewType } from '../../store/app/types'

export const CenteredLayout = styled.div`
  display: flex;
  justify-content: center;
  align-items: center;
  min-height: 100%;
  position: relative;
`

export const ToTheLeftLayout = styled.div<{ expertView: ExpertViewType }>`
  display: flex;
  justify-content: ${({ expertView }) =>
    expertView === 'open' ? 'center' : 'left'};
  flex-wrap: wrap;
  align-items: flex-start;
  position: relative;
  column-gap: ${({ theme }) => theme.spacing()};
`

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
