import styled from 'styled-components'

export const InputIcons = styled.div`
  display: flex;
  align-items: center;
  column-grid-gap: 8px;

  & > svg {
    margin-left: 2px;
    margin-right: 2px;
  }
`

export const ClickableInputIcon = styled.div`
  display: inline-flex;
  align-items: center;
  cursor: pointer;
`

export const StyledStrengthMeter = styled.span`
  transform: rotate(-90deg);
`
