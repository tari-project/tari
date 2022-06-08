import { InputProps } from './types'

import {
  StyledInput,
  IconUnitsContainer,
  InputContainer,
  UnitsText,
  IconWrapper,
  Label,
} from './styles'
import { ChangeEvent, forwardRef, useEffect, useRef, useState } from 'react'
import Text from '../../Text'
import { useTheme } from 'styled-components'

/**
 * @name Input component
 * @typedef InputProps
 *
 * @prop {boolean} [disabled] - whether component is disabled or not
 * @prop {string} [type] - input type
 * @prop {string} [value] - input text value
 * @prop {string} [id] - the input id (recommended to use when label is set)
 * @prop {string} [label] - the input label
 * @prop {string} [placeholder] - placeholder text
 * @prop {string} [inputUnits] - optional units text, e.g. 'MB' on right-hand side of input field
 * @prop {ReactNode} [inputIcon] - optional icon rendered inside input field
 * @prop {() => void} [onIconClick] - icon click event
 * @prop {(value: string) => void} [onChange] - text change event handler
 * @prop {string} [testId] - for testing purposes
 * @prop {CSSProperties} [style] - styles for actual input element
 * @prop {CSSProperties} [containerStyle] - styles for input container
 * @prop {boolean} [inverted] - use inverted styling
 */

const Input = (
  {
    autoFocus,
    type = 'text',
    value,
    id,
    label,
    disabled,
    error,
    placeholder,
    inputIcon,
    inputUnits,
    onIconClick,
    onChange,
    testId,
    style,
    containerStyle,
    inverted,
  }: InputProps,
  ref?: React.ForwardedRef<HTMLInputElement>,
) => {
  const theme = useTheme()

  const iconsRef = useRef<HTMLDivElement>(null)
  const [iconWrapperWidth, setIconWrapperWidth] = useState(22)

  useEffect(() => {
    if (iconsRef.current) {
      setIconWrapperWidth((iconsRef.current as HTMLDivElement).offsetWidth)
    }
  }, [inputIcon])

  const onChangeTextLocal = (event: ChangeEvent<HTMLInputElement>) => {
    if (onChange) {
      onChange(event.target.value)
    }
  }

  return (
    <>
      {label && (
        <Label htmlFor={id} $inverted={inverted}>
          {label}
        </Label>
      )}
      <InputContainer disabled={disabled} style={containerStyle}>
        <StyledInput
          id={id}
          autoFocus={autoFocus}
          type={type}
          placeholder={placeholder}
          disabled={disabled}
          onChange={val => onChangeTextLocal(val)}
          value={value}
          spellCheck={false}
          data-testid={testId || 'input-cmp'}
          style={style}
          ref={ref}
        />
        <IconUnitsContainer $iconWrapperWidth={iconWrapperWidth}>
          {inputIcon && (
            <IconWrapper
              onClick={disabled ? undefined : onIconClick}
              data-testid='icon-test'
              ref={iconsRef}
            >
              {inputIcon}
            </IconWrapper>
          )}{' '}
          {inputUnits && (
            <UnitsText type='smallMedium' data-testid='units-test'>
              {inputUnits}
            </UnitsText>
          )}
        </IconUnitsContainer>
      </InputContainer>
      {error && (
        <Text
          type='microMedium'
          style={{
            marginTop: theme.spacingVertical(0.2),
            fontStyle: 'italic',
            color: theme.warningDark,
          }}
        >
          {error}
        </Text>
      )}
    </>
  )
}

export default forwardRef(Input)
