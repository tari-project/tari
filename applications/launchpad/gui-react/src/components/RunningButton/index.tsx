import { useState } from 'react'
import { useTheme } from 'styled-components'

import useAnimationFrame from '../../hooks/useAnimationFrame'
import { humanizeTime } from '../../utils/Format'
import t from '../../locales'

import Text from '../Text'

import { StyledRunningButton, TextWrapper, TimeWrapper } from './styles'
import { RunningButtonProps } from './types'

const Time = ({
  startedAt,
  active,
}: {
  startedAt: number
  active?: boolean
}) => {
  const [time, setTime] = useState(humanizeTime(0))

  useAnimationFrame(() => {
    setTime(() => {
      return humanizeTime(Math.abs(Date.now() - startedAt))
    })
  }, active)

  return <>{time}</>
}

/**
 * Button with timer.
 *
 * @param {number} startedAt - timestamp in milliseconds
 * @param {boolean} [active] - should the timer be active
 * @param {() => void} onClick - on button click
 */
const RunningButton = ({
  startedAt,
  active,
  onClick,
  testId,
}: RunningButtonProps) => {
  const theme = useTheme()

  return (
    <StyledRunningButton
      type='button'
      onClick={onClick}
      data-testid={testId || 'running-button-cmp'}
    >
      <TimeWrapper>
        <Text as='span' color={theme.textSecondary} testId='timer-test-id'>
          <Time startedAt={startedAt} active={active} />
        </Text>
      </TimeWrapper>
      <TextWrapper>
        <Text
          as='span'
          color={theme.textSecondary}
          style={{ textAlign: 'center' }}
        >
          {t.common.verbs.pause}
        </Text>
      </TextWrapper>
    </StyledRunningButton>
  )
}

export default RunningButton
