import styled from 'styled-components'

export const StyledIndicatorContainer = styled.div<{
  enabled: boolean
  disabled: boolean
}>`
  color: ${({ theme, enabled, disabled }) => {
    if (disabled) {
      return theme.inputPlaceholder
    }

    if (enabled) {
      return theme.onTextLight
    }

    return theme.nodeWarningText
  }};
  display: inline-block;
  position: relative;
  &:not(:last-of-type) {
    margin-right: ${({ theme }) => theme.spacing(0.3)};
  }
`

export const EnabledDot = styled.div<{ disabled: boolean }>`
  background-color: ${({ theme, disabled }) =>
    disabled ? theme.inputPlaceholder : theme.onTextLight};
  position: absolute;
  border-radius: 50%;
  width: 3px;
  height: 3px;
  left: 50%;
  transform: translate(-50%, -75%);
`

export const ScheduleWrapper = styled.div<{ selected: boolean }>`
  width: 100%;
  box-sizing: border-box;
  padding: 0 ${({ theme }) => theme.spacing()};
  background-color: ${({ theme, selected }) =>
    selected ? theme.selectOptionHover : 'none'};
  border-radius: ${({ theme, selected }) =>
    selected ? theme.borderRadius() : 0};
  &:not(:first-of-type) > div {
    border-top: 1px solid
      ${({ theme, selected }) => (selected ? 'transparent' : theme.borderColor)};
  }
  *[data-selected='true'] + & > div {
    border-color: transparent;
  }
`

export const ScheduleContainer = styled.div`
  cursor: pointer;
  user-select: none;
  width: 100%;
  box-sizing: border-box;
  display: flex;
  padding: ${({ theme }) => theme.spacing()} 0;
  margin: 0;
  justify-content: space-between;
  align-items: center;
`

export const ScheduleInfo = styled.div``
