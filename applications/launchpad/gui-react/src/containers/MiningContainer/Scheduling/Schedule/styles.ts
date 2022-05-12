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

export const ScheduleContainer = styled.div<{ selected: boolean }>`
  width: 100%;
  box-sizing: border-box;
  border-radius: ${({ theme, selected }) =>
    selected ? theme.borderRadius() : 0};
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: ${({ theme }) => theme.spacing()};
  &:not(:first-of-type) {
    border-top: 1px solid
      ${({ theme, selected }) => (selected ? 'transparent' : theme.borderColor)};
  }
  *[data-selected='true'] + & {
    border-color: transparent;
  }
  background-color: ${({ theme, selected }) =>
    selected ? theme.backgroundImage : 'none'};
`

export const ScheduleInfo = styled.div``
