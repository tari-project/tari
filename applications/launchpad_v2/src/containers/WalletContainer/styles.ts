import styled from 'styled-components'

import SvgTariSignet from '../../styles/Icons/TariSignet'

export const Container = styled.div`
  display: flex;
  justify-content: center;
  align-items: center;
  height: 100%;
  position: relative;
`

export const TariSignet = styled(SvgTariSignet)`
  color: ${({ theme }) => theme.backgroundImage};
  height: 80px;
  width: 80px;
  position: absolute;
  right: ${({ theme }) => theme.spacing()};
  top: ${({ theme }) => theme.spacing()};
`
