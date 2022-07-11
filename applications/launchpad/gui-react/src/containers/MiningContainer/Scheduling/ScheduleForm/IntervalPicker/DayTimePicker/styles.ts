import styled from 'styled-components'

export const DayTimePickerWrapper = styled.div`
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  align-items: center;

  & .react-time-input-picker {
    display: flex;
  }

  & .react-time-input-picker input {
    border: 0;
    outline: none;
    margin: 0;
    text-align: center;
    font-family: AvenirHeavy;
    font-size: 32px;
    color: ${({ theme }) => theme.primary};
    background: inherit;

    &::-webkit-calendar-picker-indicator {
      background: none;
      display: none;
    }

    &::-webkit-datetime-edit-ampm-field {
      display: none;
    }
  }

  & .react-time-input-picker .inputWrapper:nth-child(2)::before {
    content: ':';
    font-size: 32px;
    font-family: AvenirHeavy;
    color: ${({ theme }) => theme.primary};
  }

  &
    .react-time-input-picker
    .inputWrapper:first-of-type
    input::-webkit-outer-spin-button,
  &
    .react-time-input-picker
    .inputWrapper:first-of-type
    input::-webkit-inner-spin-button,
  &
    .react-time-input-picker
    .inputWrapper:nth-child(2)
    input::-webkit-outer-spin-button,
  &
    .react-time-input-picker
    .inputWrapper:nth-child(2)
    input::-webkit-inner-spin-button {
    -webkit-appearance: none;
    margin: 0;
  }

  & .react-time-input-picker .inputWrapper:nth-child(3) {
    display: none !important;
  }
`
