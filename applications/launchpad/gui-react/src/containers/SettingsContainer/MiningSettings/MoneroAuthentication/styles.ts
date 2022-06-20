import styled from 'styled-components'

export const ModalContainer = styled.div`
  display: flex;
  height: 100%;
  flex-direction: column;
`

export const ModalContent = styled.div`
  overflow: auto;
  flex: 1;
  padding: ${({ theme }) => theme.spacingHorizontal(1.6)};
`

export const ModalFooter = styled.div`
  display: flex;
  padding: ${({ theme }) =>
    `${theme.spacingVertical(2)} ${theme.spacingHorizontal(1.6)}`};
  gap: ${({ theme }) => theme.spacingHorizontal(1)};
  border-top: 1px solid ${({ theme }) => theme.borderColor};

  & > button:last-child {
    flex: 1;
    justify-content: center;
  }
`

export const Description = styled.div`
  margin: ${({ theme }) => `${theme.spacingHorizontal(1.6)} 0`};
`

export const InputWrapper = styled.div`
  margin: ${({ theme }) => `${theme.spacingHorizontal(1.6)} 0`};
`
