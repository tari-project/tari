import { Fragment, ReactNode } from 'react'
import { Listbox } from '@headlessui/react'

import Text from '../Text'

import ArrowBottom from '../../styles/Icons/ArrowBottom1'

import {
  Label,
  SelectButton,
  SelectorIcon,
  OptionsContainer,
  Option,
} from './styles'
import { Option as OptionProp, SelectStylesOverrideProps } from './types'

/**
 * @name Select
 *
 * Renders a tari-styled single select
 *
 * @prop {boolean?} inverted - whether component should display inverted styles on dark background
 * @prop {string} [label] - optional label used for component
 * @prop {Option[]} options - options shown in the select dropdown
 * @prop {Option} value - selected value
 * @prop {function} onChange - called when selected value changes
 * @prop {boolean?} disabled - disables the the control
 * @prop {ReactNode} [icon] - icon to show left to the selected value
 * @prop {SelectStylesOverrideProps} [styles] - optional style overrides for Select
 * @prop {boolean} [fullWidth] - default: true, with this select renders as 100% of container width
 */
const Select = ({
  value,
  options,
  onChange,
  inverted,
  label,
  disabled,
  styles,
  icon,
  fullWidth = true,
}: {
  disabled?: boolean
  inverted?: boolean
  label?: string
  value?: OptionProp
  options: OptionProp[]
  onChange: (option: OptionProp) => void
  styles?: SelectStylesOverrideProps
  icon?: ReactNode
  fullWidth?: boolean
}) => {
  return (
    <Listbox value={value} onChange={onChange} disabled={disabled}>
      {({ open }: { open: boolean }) => (
        <div>
          {label && (
            <Label inverted={inverted} style={{ ...styles?.label }}>
              {label}
            </Label>
          )}
          <SelectButton
            open={open}
            inverted={inverted}
            disabled={disabled}
            fullWidth={fullWidth}
            style={{ ...styles?.value }}
          >
            {icon}
            <Text as='span' type='smallMedium' color='inherit'>
              {(value || {}).label || ''}
            </Text>
            {!disabled && (
              <SelectorIcon inverted={inverted} style={{ ...styles?.icon }}>
                <ArrowBottom />
              </SelectorIcon>
            )}
          </SelectButton>
          <OptionsContainer inverted={inverted} fullWidth={fullWidth}>
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
        </div>
      )}
    </Listbox>
  )
}

export default Select
