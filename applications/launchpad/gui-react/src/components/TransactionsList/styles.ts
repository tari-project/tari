import styled from 'styled-components'

export const DirectionTag = styled.span<{ $variant: 'earned' | 'out' }>`
  background: ${({ $variant, theme }) =>
    $variant === 'earned' ? theme.on : theme.warning};
  color: ${({ $variant, theme }) =>
    $variant === 'earned' ? theme.onText : theme.warningText};
  border-radius: ${({ theme }) => theme.borderRadius(0.5)};
  display: flex;
  align-items: center;
  justify-content: center;
  height: ${({ theme }) => theme.spacing(1)};
  width: ${({ theme }) => theme.spacing(1)};
`

export const EmojiWrapper = styled.span`
  font-size: 12px;
`

export const StyledAddress = styled.span`
  text-decoration: underline;
  color: ${({ theme }) => theme.accentDark};
`

export const StyledTable = styled.table`
  width: 100%;
  border-collapse: collapse;

  & tr {
    border-bottom: 1px solid ${({ theme }) => theme.borderColor};

    &:first-child {
      border-top: 1px solid ${({ theme }) => theme.borderColor};
    }

    & td {
      padding: ${({ theme }) =>
        `${theme.spacingVertical(1.23)} ${theme.spacingHorizontal(0.333)}`};
    }
  }
`

export const AmountTd = styled.td<{
  $variant: 'earned' | 'out'
  $inverted: boolean
}>`
  text-align: right;
  max-width: ${({ theme }) => theme.spacingHorizontal(4)};
  box-sizing: border-box;

  & > * {
    color: ${({ $variant, theme, $inverted }) =>
      $variant === 'earned'
        ? $inverted
          ? theme.accent
          : theme.onTextLight
        : $inverted
        ? theme.primary
        : theme.secondary};
  }
`

export const DateTd = styled.td`
  max-width: ${({ theme }) => theme.spacingHorizontal(3)};
  box-sizing: border-box;
  text-align: center;

  & > span {
    color: ${({ theme }) => theme.secondary};
  }
`

export const DirectionTd = styled.td`
  max-width: ${({ theme }) => theme.spacingHorizontal()};
  box-sizing: border-box;
`

export const EventTd = styled.td<{ $inverted: boolean }>`
  padding-top: ${({ theme }) => theme.spacingVertical(1.6)} !important;
  & > span {
    line-height: 20px !important;
  }
  color: ${({ theme, $inverted }) => ($inverted ? theme.secondary : '')};
`

export const StatusTd = styled.td`
  max-width: ${({ theme }) => theme.spacingHorizontal(3)};
  box-sizing: border-box;

  & > div {
    margin: auto;
  }
`
