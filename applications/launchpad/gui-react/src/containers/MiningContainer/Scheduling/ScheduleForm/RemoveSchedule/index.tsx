import { useTheme } from 'styled-components'

import TrashIcon from '../../../../../styles/Icons/Trash2'
import Text from '../../../../../components/Text'
import Box from '../../../../../components/Box'
import Button from '../../../../../components/Button'

const RemoveSchedule = ({ remove }: { remove: () => void }) => {
  const theme = useTheme()
  return (
    <Box
      border={false}
      style={{ marginBottom: 0, paddingBottom: 0, paddingTop: 0, marginTop: 0 }}
    >
      <Button
        variant='text'
        leftIcon={
          <TrashIcon width='1em' height='1em' color={theme.secondary} />
        }
        onClick={remove}
        style={{ paddingLeft: 0 }}
      >
        <Text as='label' color={theme.secondary} style={{ cursor: 'pointer' }}>
          Remove schedule
        </Text>
      </Button>
    </Box>
  )
}

export default RemoveSchedule
