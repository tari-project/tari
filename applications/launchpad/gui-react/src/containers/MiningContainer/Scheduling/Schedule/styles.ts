import styled from 'styled-components'

export const StyledIndicatorContainer = styled.div<{
  enabled: boolean
  disabled: boolean
}>`
  color: ${({ theme, enabled, disabled }) => {
    if (disabled) {
      return theme.placeholderText
    }

    if (enabled) {
      return theme.onTextLight
    }

    return theme.secondary
  }};
  display: inline-block;
  position: relative;
  &:not(:last-of-type) {
    margin-right: ${({ theme }) => theme.spacing(0.3)};
  }
`

export const EnabledDot = styled.div<{ disabled: boolean }>`
  background-color: ${({ theme, disabled }) =>
    disabled ? theme.placeholderText : theme.onTextLight};
  position: absolute;
  border-radius: 50%;
  width: 3px;
  height: 3px;
  left: 50%;
  transform: translate(-50%, -75%);
`

export const ScheduleContainer = styled.div`
  width: 100%;
  margin-top: ${({ theme }) => theme.spacing(0.5)};
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 0 ${({ theme }) => theme.spacing()};
  box-sizing: border-box;
  &:not(:last-of-type) {
    border-bottom: 1px solid ${({ theme }) => theme.borderColor};
    padding-bottom: ${({ theme }) => theme.spacing()};
    margin-bottom: ${({ theme }) => theme.spacing()};
  }
`

export const ScheduleInfo = styled.div``
