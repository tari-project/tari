/* eslint-disable indent */
import styled from 'styled-components'
import Text from '../../Text'

export const StyledInput = styled.input<{
  type?: string
  value?: string
  hideText?: boolean
}>`
  height: 100%;
  width: 100%;
  padding: 0px 16px;
  font-family: 'AvenirMedium';
  font-size: 14px;
  color: ${({ theme, type, value, hideText }) => {
    if (type === 'disabled') {
      return theme.placeholderText
      // TODO: pre-filled input requires secondary text color
    } else if (hideText && value && value !== '') {
      return 'transparent'
    } else {
      return theme.primary
    }
  }};
  background-color: ${({ theme, type }) =>
    type === 'disabled' ? theme.backgroundImage : theme.background};
  border: none;
  border-radius: 8px;
  ::placeholder {
    color: ${({ theme }) => theme.placeholderText};
  }
  &:focus {
    outline: none;
    color: ${({ theme, hideText, value }) => {
      if (hideText && value && value !== '') {
        return 'transparent'
      } else {
        return theme.primary
      }
    }};
  }
`

export const InputContainer = styled.div<{ type?: string }>`
  height: 42px;
  width: 369px;
  display: flex;
  align-items: center;
  background-color: ${({ theme, type }) =>
    type === 'disabled' ? theme.backgroundImage : theme.background};
  border: 1px solid;
  border-color: ${({ theme }) => theme.borderColor};
  border-radius: 8px;
  /* @TODO: "highlighted vs. active input states from designs to be looked at again" */
  /* box-shadow: 0px 0px 0px 2px
    ${({ type }) => (type === 'highlighted' ? '#F5EBFF' : 'transparent')}; */
  font-family: 'AvenirMedium';
  :focus-within {
    outline: none;
    border-color: ${({ theme }) => theme.accent};
  }
`

export const IconUnitsContainer = styled.div`
  width: 22px;
  height: auto;
  display: flex;
  justify-content: center;
  align-items: center;
  margin-right: 10px;
`

export const IconWrapper = styled.div`
  display: flex;
  font-size: 20px;
  color: ${({ theme }) => theme.secondary};
`

export const UnitsText = styled(Text)`
  color: ${({ theme }) => theme.placeholderText};
  text-transform: uppercase;
`
