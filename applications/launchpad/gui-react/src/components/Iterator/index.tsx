import { useTheme } from 'styled-components'

import IconButton from '../IconButton'
import Text from '../Text'
import ArrowLeft from '../../styles/Icons/ArrowLeft2'
import ArrowRight from '../../styles/Icons/ArrowRight2'

import { Wrapper } from './styles'

/**
 * @name Iterator
 * @description controlled presentation component for iterating over any value with next/previous buttons for the user
 *
 * @prop {string} value - current value
 * @prop {() => void} next - callback for going to next value
 * @prop {() => void} previous - callback for going to previous value
 */
const Iterator = ({
  value,
  next,
  previous,
}: {
  value: string
  next: () => void
  previous: () => void
}) => {
  const theme = useTheme()

  return (
    <Wrapper>
      <IconButton
        testId='iterator-btn-prev'
        onClick={previous}
        style={{
          color: theme.secondary,
          marginBottom: '-3px',
          marginRight: theme.spacing(0.5),
        }}
      >
        <ArrowLeft width='28px' height='28px' />
      </IconButton>
      <Text color={theme.secondary} style={{ marginBottom: '-3px' }}>
        {value}
      </Text>
      <IconButton
        testId='iterator-btn-next'
        onClick={next}
        style={{
          color: theme.secondary,
          marginBottom: '-3px',
          marginLeft: theme.spacing(0.5),
        }}
      >
        <ArrowRight width='28px' height='28px' />
      </IconButton>
    </Wrapper>
  )
}

export default Iterator
