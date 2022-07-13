import styled from 'styled-components'

export const InputContainer = styled.div`
  display: flex;
  align-items: center;
  justify-content: center;
`

export const IconWrapper = styled.div`
  margin-right: ${({ theme }) => theme.spacingHorizontal(0.42)};
`

export const InputWrapper = styled.span`
  overflow: hidden;
  padding: 0 4px 0 6px;
  box-sizing: border-box;
  max-width: 50%;
  margin-top: 6px;
`

export const StyledAmountInput = styled.div``

export const StyledInput = styled.input`
  padding: 0px 6px;
  font-family: 'AvenirMedium';
  font-size: 48px;
  line-height: 66px;
  font-weight: bold;
  flex: 1;
  width: 100%;
  border: none;
  text-align: right;
  box-sizing: border-box;
  color: ${({ theme }) => theme.primary};

  :focus-within {
    outline: none;
    border-color: ${({ theme }) => theme.accent};
  }
`

export const Currency = styled.span`
  margin-top: 20px;
`

export const TransactionFee = styled.div`
  display: flex;
  align-items: center;
  justify-content: center;
  padding: ${({ theme }) => theme.spacing(0.25)};
  column-gap: ${({ theme }) => theme.spacing(0.25)};
  min-height: 30px;
  box-sizing: border-box;
`

export const ErrorContainer = styled.div`
  display: flex;
  justify-content: center;
  min-height: 25px;
  padding-top: ${({ theme }) => theme.spacing(0.075)};
  padding-bottom: ${({ theme }) => theme.spacing(0.125)};
  padding-left: ${({ theme }) => theme.spacing(0.25)};
  padding-right: ${({ theme }) => theme.spacing(0.25)};
`
