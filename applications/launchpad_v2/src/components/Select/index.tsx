import { Fragment } from 'react'
import { Listbox } from '@headlessui/react'

import ArrowBottom from '../../styles/Icons/ArrowBottom1'

import { Label, SelectButton, SelectorIcon, OptionsContainer, Option } from './styles'
import { MyListboxProps } from './types'

/**
 * @name Select
 *
 * Renders a tari-styled single select
 *
 * @prop {boolean?} invertedStyle - whether component should display inverted styles on dark background
 * @prop {string} label - label used for component
 * @prop {Option[]} options - options shown in the select dropdown
 * @prop {Option} value - selected value
 * @prop {function} onChange - called when selected value changes
 *
 * @typedef Option
 * @prop {string} value - value of the option
 * @prop {string} label - label shown in option
 * @prop {string} key - key to be used in react map
 */
const Select = ({ value, options, onChange, invertedStyle, label }: MyListboxProps) => {
  return (
    <Listbox value={value} onChange={onChange}>
      {({ open }) => <>
        <Label darkBackground={invertedStyle}>{label}</Label>
        <SelectButton open={open} inverted={invertedStyle}>
          <span>{value?.label || ''}</span>
          <SelectorIcon inverted={invertedStyle}>
            <ArrowBottom />
          </SelectorIcon>
        </SelectButton>
        <OptionsContainer inverted={invertedStyle}>
          {options.map((option) => (
            <Listbox.Option key={option.key} value={option} as={Fragment}>
              {({ active, selected }) => (
                <Option selected={selected} active={active} inverted={invertedStyle}>
                  {option.label}
                </Option>
              )}
            </Listbox.Option>
          ))}
        </OptionsContainer>
      </>}
    </Listbox>
  )
}

export default Select
