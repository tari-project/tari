import styled from 'styled-components'

export const SetupMergedContent = styled.div`
  display: flex;
  flex-direction: column;
  flex: 1;
  justify-content: space-between;
  align-items: flex-start;
`

export const SetupMergedFormContainer = styled.div`
  margin-top: ${({ theme }) => theme.spacingVertical(3)};
`

export const FormTextWrapper = styled.div`
  padding-top: ${({ theme }) => theme.spacingVertical()};
  padding-bottom: ${({ theme }) => theme.spacingVertical()};
  margin-bottom: ${({ theme }) => theme.spacingVertical()};
`

export const StyledBestChoiceTag = styled.span`
  display: flex;
  align-items: center;
  flex: 1;
`

export const BestChoiceTagText = styled.span`
  display: inline-flex;
`

export const BestChoiceTagIcon = styled.span`
  font-size: 0.84em;
  line-height: 0.4em;
  position: relative;
  display: inline-flex;
  marginleft: 4px;
`
