import styled from 'styled-components'

export const MiningBoxContent = styled.div`
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  justify-content: space-between;
  flex: 1;
`

export const NodeIcons = styled.div<{ $color: string }>`
  position: absolute;
  width: 80px;
  min-height: 80px;
  right: ${({ theme }) => theme.spacing()};
  top: ${({ theme }) => theme.spacing()};
  color: ${({ $color }) => $color};

  & > * {
    width: 80px;
    height: 80px;
    color: inherit;
    margin-bottom: ${({ theme }) => theme.spacing(0.4)};
  }
`
