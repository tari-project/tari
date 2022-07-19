import { useAppDispatch } from '../../../../store/hooks'
import { tbotactions } from '../../../../store/tbot'
import t from '../../../../locales'
import Button from '../../../Button'

const GotItButton = () => {
  const dispatch = useAppDispatch()

  const close = () => {
    return dispatch(tbotactions.close())
  }

  return (
    <div>
      <Button
        type='button'
        variant='primary'
        onClick={close}
        size='medium'
        testId='gotitbutton-cmp'
      >
        {`${t.common.phrases.gotIt}!`}
      </Button>
    </div>
  )
}

export default GotItButton
