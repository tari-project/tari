import { clipboard } from '@tauri-apps/api'

import Button from '../Button'
import Text from '../Text'
import CopyIcon from '../../styles/Icons/Copy'

import { StyledBox } from './styles'

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
