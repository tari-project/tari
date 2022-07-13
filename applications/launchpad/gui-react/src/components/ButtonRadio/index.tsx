import { useTheme } from 'styled-components'

import Button from '../Button'

/**
 * @name ButtonRadio
 * @description controlled presentation component that shows a row of buttons and allows to select only one of them at a time
 *
 * @prop {string} value - currently selected value
 * @prop {{ option: string; label: string; disabled?: boolean }[]} options - options to be rendered as buttons
 * @prop {(option: string) => void} onChange - value change callback
 */
const ButtonRadio = ({
  value,
  options,
  onChange,
}: {
  value: string
  options: { option: string; label: string; disabled?: boolean }[]
  onChange: (option: string) => void
}) => {
  const theme = useTheme()

  if (options.length === 0) {
    return null
  }

  return (
    <div style={{ display: 'flex', columnGap: theme.spacing(0.5) }}>
      {options.map(({ option, label, disabled }) => (
        <Button
          disabled={disabled}
          key={option}
          variant='secondary'
          style={{
            backgroundColor:
              value === option
                ? theme.disabledPrimaryButton
                : theme.buttonRadioBackground,
            color: theme.nodeWarningText,
            borderColor:
              value === option
                ? theme.buttonRadioBorder
                : theme.selectBorderColor,
          }}
          onClick={() => onChange(option)}
        >
          {label}
        </Button>
      ))}
    </div>
  )
}

export default ButtonRadio
