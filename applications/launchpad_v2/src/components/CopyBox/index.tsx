import { useTheme } from 'styled-components'
import { clipboard } from '@tauri-apps/api'

import Button from '../Button'
import Text from '../Text'
import CopyIcon from '../../styles/Icons/Copy'

const CopyBox = ({ label, value }: { label: string; value: string }) => {
  const theme = useTheme()

  const copy = async () => {
    await clipboard.writeText(value)

    alert(`copied ${value}`)
  }

  return (
    <>
      <Text>{label}</Text>
      <span
        style={{
          background: theme.backgroundImage,
          border: `1px solid ${theme.borderColor}`,
          borderRadius: theme.tightBorderRadius(),
          color: theme.secondary,
          padding: `${theme.spacingVertical()} ${theme.spacingHorizontal()}`,
          margin: `${theme.spacingVertical()} 0`,
          boxSizing: 'border-box',
          display: 'flex',
          justifyContent: 'space-between',
        }}
      >
        {value}
        <Button variant='text' style={{ padding: 0, margin: 0 }} onClick={copy}>
          <CopyIcon />
        </Button>
      </span>
    </>
  )
}

export default CopyBox
