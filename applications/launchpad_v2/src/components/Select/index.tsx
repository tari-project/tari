import { Fragment } from 'react'
import { Listbox } from '@headlessui/react'

import Text from '../Text'

import ArrowBottom from '../../styles/Icons/ArrowBottom1'

import {
  StyledListbox,
  Label,
  SelectButton,
  SelectorIcon,
  OptionsContainer,
  Option,
} from './styles'
import { Option as OptionProp } from './types'

/**
 * @TODO go back to import SelectProps - it was switched, because eslint was giving some react/prop-types error
 */

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
}: {
  disabled?: boolean
  inverted?: boolean
  label: string
  value?: OptionProp
  options: OptionProp[]
  onChange: (option: OptionProp) => void
}) => {
  return (
    <StyledListbox value={value} onChange={onChange} disabled={disabled}>
      {({ open }: { open: boolean }) => (
        <>
          <Label inverted={inverted}>{label}</Label>
          <SelectButton open={open} inverted={inverted} disabled={disabled}>
            <Text as='span' type='smallMedium' color='inherit'>
              {(value || {}).label || ''}
            </Text>
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
                    <Text as='span' type='smallMedium' color='inherit'>
                      {option.label}
                    </Text>
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
