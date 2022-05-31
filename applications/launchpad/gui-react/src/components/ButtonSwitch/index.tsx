import { useTheme } from 'styled-components'

import Button from '../Button'

const ButtonSwitch = ({
  value,
  options,
  onChange,
}: {
  value: string
  options: { option: string; label: string }[]
  onChange: (option: string) => void
}) => {
  const theme = useTheme()

  return (
    <div style={{ display: 'flex', columnGap: theme.spacing(0.5) }}>
      {options.map(({ option, label }) => (
        <Button
          key={option}
          variant='secondary'
          style={{
            backgroundColor: value === option ? '' : 'transparent',
            color: theme.secondary,
          }}
          onClick={() => onChange(option)}
        >
          {label}
        </Button>
      ))}
    </div>
  )
}

export default ButtonSwitch
