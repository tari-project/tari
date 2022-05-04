import { clipboard } from '@tauri-apps/api'

import Button from '../Button'
import Text from '../Text'
import CopyIcon from '../../styles/Icons/Copy'

import { StyledBox } from './styles'

/**
 * @name CopyBoc
 * @description Renders a box with value with a button that allows to copy it
 *
 *
 * @prop {string} label - label describing the value
 * @prop {string} value - value that can be copied to clipboard
 */
const CopyBox = ({ label, value }: { label: string; value: string }) => {
  const copy = async () => {
    await clipboard.writeText(value)

    alert(`copied ${value}`)
  }

  return (
    <>
      <Text>{label}</Text>
      <StyledBox>
        {value}
        <Button variant='text' style={{ padding: 0, margin: 0 }} onClick={copy}>
          <CopyIcon />
        </Button>
      </StyledBox>
    </>
  )
}

export default CopyBox
