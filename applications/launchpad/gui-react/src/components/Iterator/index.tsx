import { CSSProperties } from 'react'
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
 * @prop {CSSProperties} style - wrapper style overrides
 */
const Iterator = ({
  value,
  next,
  previous,
  hasNext,
  hasPrevious,
  style,
}: {
  value: string
  next: () => void
  previous: () => void
  hasNext?: boolean
  hasPrevious?: boolean
  style?: CSSProperties
}) => {
  const theme = useTheme()

  const disableNextButton = hasNext !== undefined && !hasNext
  const disablePreviousButton = hasPrevious !== undefined && !hasPrevious

  return (
    <Wrapper style={style}>
      <IconButton
        disabled={disablePreviousButton}
        testId='iterator-btn-prev'
        onClick={previous}
        style={{
          color: theme.nodeWarningText,
          marginBottom: '-3px',
          marginRight: theme.spacing(0.5),
        }}
      >
        <ArrowLeft width='28px' height='28px' />
      </IconButton>
      <Text color={theme.nodeWarningText} style={{ marginBottom: '-3px' }}>
        {value}
      </Text>
      <IconButton
        disabled={disableNextButton}
        testId='iterator-btn-next'
        onClick={next}
        style={{
          color: theme.nodeWarningText,
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
