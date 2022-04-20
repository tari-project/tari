import { useEffect, useState, Fragment } from 'react'
import { invoke } from '@tauri-apps/api/tauri'
import styled from 'styled-components'
import { Listbox } from '@headlessui/react'

import ArrowBottom from '../../styles/Icons/ArrowBottom1'
import { useTheme } from '../../styles'

const WithTheme = (Component) => function TT(props) {
  const theme = useTheme()

  return <Component {...props} theme={theme} />
}

const SelectorIcon = WithTheme(styled.div`
position: absolute;
top: 0;
right: ${({ theme }) => theme.spacingHorizontal(0.5)};
height: 100%;
display: flex;
flex-direction: column;
justify-content: center;
font-size: 1.5em;
color: ${({ theme, onDark }) => onDark ? theme.background : theme.primary};
`)

const SelectButton = WithTheme(styled(Listbox.Button)`
font-size: 1em;
color: ${({ theme, onDark }) => onDark ? theme.background : theme.primary};
position: relative;
width: ${({ fullWidth }) => fullWidth ? '100%' : 'auto'};
appearance: none;
background-color: ${({ onDark }) => onDark ? 'rgba(255,255,255,.2)' : 'transparent'} ;
padding: 0;
padding: ${({ theme }) => `${theme.spacingVertical()} ${theme.spacingHorizontal()}`};
padding-right: ${({ theme }) => theme.spacingHorizontal()};
margin: 0;
outline: none;
border: none;
border: 1px solid;
border-radius: ${({ theme }) => theme.borderRadius()};
border-color: ${({ theme, onDark, open }) => open ? (onDark ? theme.background : theme.accent) : theme.borderColor};
text-align: left;
`)

const OptionsContainer = WithTheme(styled(Listbox.Options)`
position: relative;
margin: 0;
margin-top: ${({ theme }) => theme.spacingVertical()};
padding: 0;
width: ${({ fullWidth }) => fullWidth ? '100%' : 'auto'};
outline: none;
`)

const FloatingOptions = WithTheme(styled.ul`
color: ${({ theme, onDark }) => onDark ? theme.background : theme.primary};
position: absolute;
margin: 0;
padding: 0;
width: 100%;
border: 1px solid;
border-radius: ${({ theme }) => theme.borderRadius()};
border-color: ${({ theme, open }) => open ? theme.accent : theme.borderColor};
background-color: ${({ theme }) => theme.background};
`)

const Option = WithTheme(styled.li`
list-style-type: none;
position: relative;
padding: ${({ theme }) => `${theme.spacingVertical(0.5)} ${theme.spacingHorizontal(0.5)}`};
margin: ${({ theme }) => `${theme.spacingVertical(0.5)} ${theme.spacingHorizontal(0.5)}`};
border-radius: ${({ theme }) => theme.borderRadius(.5)};
background-color: ${({ theme, selected, active }) => selected || active ? theme.selected : 'transparent'};
outline: none;
cursor: default;

&:hover {
  background-color: ${({ theme }) => theme.selected};
}
`)

const Label = WithTheme(styled(Listbox.Label)`
font-size: 1em;
display: inline-block;
margin-bottom: ${({ theme }) => theme.spacingVertical()};
color: ${({ theme, onDark }) => onDark ? theme.background : theme.primary};
`)

type Option = { value: string; label: string; key: string; }
type MyListboxProps = { fullWidth?: boolean; onDark?: boolean; label: string; value: Option; options: Option[]; onChange: (option: Option) => void }
function MyListbox({ value, options, onChange, fullWidth, onDark, label }: MyListboxProps) {
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
          <FloatingOptions>
            {options.map((option) => (
              <Listbox.Option key={option.key} value={option} as={Fragment}>
                {({ active, selected }) => (
                  <Option selected={selected} active={active} onDark={onDark}>
                    {option.label}
                  </Option>
                )}
              </Listbox.Option>
            ))}
          </FloatingOptions>
        </OptionsContainer>
      </>}
    </Listbox>
  )
}

const networks = ['mainnet', 'testnet']
const networkOptions = networks.map(network => ({
  label: network,
  value: network,
  key: network,
}))

const BaseNodeContainer = () => {
  const [images, setImages] = useState<string[]>([])
  const [tariNetwork, setTariNetwork] = useState(networkOptions[0])

  useEffect(() => {
    const getFromBackend = async () => {
      const imagesFromBackend = await invoke<string[]>('image_list')
      setImages(imagesFromBackend)
    }

    getFromBackend()
  }, [])

  return (
    // <div style={{ backgroundColor: '#662FA1', padding: '2em'}}>
    <div>
      <h2>Base Node</h2>
      <MyListbox value={tariNetwork} options={networkOptions} onChange={setTariNetwork} label="Tari network" fullWidth/>
      <p>
        available docker images:
        <br />
        {images.map(img => (
          <em key={img}>
            {img}
            {', '}
          </em>
        ))}
      </p>
    </div>
  )
}

export default BaseNodeContainer
