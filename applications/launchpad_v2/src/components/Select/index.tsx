import { Fragment } from 'react'
import { Listbox } from '@headlessui/react'

import ArrowBottom from '../../styles/Icons/ArrowBottom1'

import {
  StyledListbox,
  Label,
  SelectButton,
  SelectorIcon,
  OptionsContainer,
  Option,
} from './styles'
import { SelectProps } from './types'

/**
 * @name Select
 *
 * Renders a tari-styled single select
 *
 * @prop {boolean?} inverted - whether component should display inverted styles on dark background
 * @prop {string} label - label used for component
 * @prop {Option[]} options - options shown in the select dropdown
 * @prop {Option} value - selected value
 * @prop {function} onChange - called when selected value changes
 * @prop {boolean?} disabled - disables the the control
 *
 * @typedef Option
 * @prop {string} value - value of the option
 * @prop {string} label - label shown in option
 * @prop {string} key - key to be used in react map
 */
const Select = ({
  value,
  options,
  onChange,
  inverted,
  label,
  disabled,
}: SelectProps) => {
  return (
    <StyledListbox value={value} onChange={onChange} disabled={disabled}>
      {({ open }) => (
        <>
          <Label inverted={inverted}>{label}</Label>
          <SelectButton open={open} inverted={inverted} disabled={disabled}>
            <span>{(value || {}).label || ''}</span>
            {!disabled && (
              <SelectorIcon inverted={inverted}>
                <ArrowBottom />
              </SelectorIcon>
            )}
          </SelectButton>
          <OptionsContainer inverted={inverted}>
            {options.map(option => (
              <Listbox.Option key={option.key} value={option} as={Fragment}>
                {({ active, selected }) => (
                  <Option
                    selected={selected}
                    active={active}
                    inverted={inverted}
                  >
                    {option.label}
                  </Option>
                )}
              </Listbox.Option>
            ))}
          </OptionsContainer>
        </>
      )}
    </StyledListbox>
  )
}

export default Select
