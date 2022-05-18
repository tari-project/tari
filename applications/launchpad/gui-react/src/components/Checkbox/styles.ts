import styled from 'styled-components'

export const Wrapper = styled.div`
  display: flex;
  alignyitems: baseline;
`

export const CheckWrapper = styled.div<{ checked: boolean }>`
  display: flex;
  justify-content: center;
  align-items: center;
  width: 1em;
  height: 1em;
  border: 2px solid
    ${({ checked, theme }) => (checked ? theme.accent : theme.secondary)};
  border-radius: 3px;
  margin-right: ${({ theme }) => theme.spacing(0.5)};
  cursor: pointer;
`
