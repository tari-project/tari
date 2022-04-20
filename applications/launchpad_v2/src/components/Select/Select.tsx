import { Fragment } from 'react'
import { Listbox } from '@headlessui/react'

import ArrowBottom from '../../styles/Icons/ArrowBottom1'

import {Label, SelectButton, SelectorIcon, OptionsContainer, Option} from './styledComponents'

type Option = { value: string; label: string; key: string; }
type MyListboxProps = { fullWidth?: boolean; onDark?: boolean; label: string; value: Option; options: Option[]; onChange: (option: Option) => void }

export function Select({ value, options, onChange, fullWidth, onDark, label }: MyListboxProps) {
  return (
    <Listbox value={value} onChange={onChange}>
      {({ open }) => <>
        <Label onDark={onDark}>{label}</Label>
        <SelectButton open={open} fullWidth={fullWidth} onDark={onDark}>
          <span>{value.label}</span>
          <SelectorIcon onDark={onDark}>
            <ArrowBottom />
          </SelectorIcon>
        </SelectButton>
        <OptionsContainer fullWidth={fullWidth} onDark={onDark}>
          {options.map((option) => (
            <Listbox.Option key={option.key} value={option} as={Fragment}>
              {({ active, selected }) => (
                <Option selected={selected} active={active} onDark={onDark}>
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
