import { useAppDispatch } from '../../../../store/hooks'
import { tbotactions } from '../../../../store/tbot'
import Button from '../../../Button'

const GotItButton = () => {
  const dispatch = useAppDispatch()
  return (
    <Button
      type='button'
      variant='primary'
      onClick={() => dispatch(tbotactions.close())}
      size='medium'
    >
      Got it!
    </Button>
  )
}

export default GotItButton
