import Button from '../../../Button'

const GotItButton = ({ onClick }: { onClick: () => void }) => {
  return (
    <Button
      type='button'
      variant='primary'
      onClick={onClick}
      size='medium'
      testId='gotitbutton-cmp'
    >
      Got it!
    </Button>
  )
}

export default GotItButton
