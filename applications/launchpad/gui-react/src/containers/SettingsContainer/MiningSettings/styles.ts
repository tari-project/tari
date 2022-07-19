import styled from 'styled-components'

export const AddressDescription = styled.div`
  & > p {
    color: ${({ theme }) => theme.nodeWarningText};
  }
`

export const NarrowInlineInput = styled.div`
  display: flex;
  align-items: center;
  justify-content: flex-start;
  gap: ${({ theme }) => theme.spacingHorizontal(0.5)};
  margin: ${({ theme }) => theme.spacingVertical(0.5)} 0;

  & > label {
    margin-bottom: 0;
  }

  & input {
    max-width: 96px;
    min-width: auto;
  }
`

export const ActionsContainer = styled.div`
  color: ${({ theme }) => theme.greenMedium};
  display: flex;
  justify-content: flex-end;
  padding-top: ${({ theme }) => theme.spacingVertical(1)};
  padding-bottom: ${({ theme }) => theme.spacingVertical(1)};
  margin-bottom: ${({ theme }) => theme.spacingVertical(2)};

  & > button {
    text-decoration: none;
  }
`

export const UrlList = styled.div`
  margin-top: ${({ theme }) => theme.spacingVertical(1)};
`
