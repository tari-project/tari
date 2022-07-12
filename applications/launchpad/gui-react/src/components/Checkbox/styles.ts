import styled from 'styled-components'

export const Wrapper = styled.div<{ disabled?: boolean }>`
  display: flex;
  pointer-events: ${({ disabled }) => (disabled ? 'none' : 'auto')};
`

export const CheckWrapper = styled.div<{
  checked: boolean
  disabled?: boolean
}>`
  display: flex;
  justify-content: center;
  align-items: center;
  width: 1em;
  height: 1em;
  border: 2px solid
    ${({ disabled, checked, theme }) => {
      if (disabled) {
        return theme.placeholderText
      }
      return checked ? theme.accent : theme.nodeWarningText
    }};
  border-radius: 3px;
  margin-right: ${({ theme }) => theme.spacing(0.5)};
  cursor: ${({ disabled }) => (disabled ? 'default' : 'pointer')};
`
