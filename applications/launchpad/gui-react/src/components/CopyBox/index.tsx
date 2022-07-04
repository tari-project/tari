import { CSSProperties, useState, useEffect, useRef } from 'react'
import { clipboard } from '@tauri-apps/api'
import { useSpring, animated } from 'react-spring'

import Button from '../Button'
import Text from '../Text'
import Tag from '../Tag'
import CopyIcon from '../../styles/Icons/Copy'
import t from '../../locales'

import { StyledBox, FeedbackContainer } from './styles'

/**
 * @name CopyBox
 * @description Renders a box with value with a button that allows to copy it
 *
 *
 * @prop {string} [label] - label describing the value
 * @prop {string} value - value that can be copied to clipboard
 */
const CopyBox = ({
  label,
  value,
  style,
}: {
  label?: string
  value: string
  style?: CSSProperties
}) => {
  const [copied, setCopied] = useState(false)
  const styles = useSpring({ opacity: copied ? 1 : 0 })
  const timeout = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)

  const copy = async () => {
    await clipboard.writeText(value)

    setCopied(true)
    if (timeout.current) {
      clearTimeout(timeout.current)
    }

    timeout.current = setTimeout(() => {
      setCopied(false)
      timeout.current = undefined
    }, 2000)
  }

  useEffect(() => {
    return () => {
      if (timeout.current) {
        clearTimeout(timeout.current)
      }
    }
  }, [])

  return (
    <>
      {label && <Text>{label}</Text>}
      <StyledBox style={style}>
        <div
          style={{
            overflowX: 'hidden',
            textOverflow: 'ellipsis',
            wordBreak: 'keep-all',
          }}
          title={value}
        >
          {value}
        </div>
        <Button
          variant='text'
          style={{
            padding: 0,
            margin: 0,
            position: 'relative',
            flexGrow: '1',
            color: 'inherit',
          }}
          onClick={copy}
        >
          <FeedbackContainer>
            <animated.div style={styles}>
              <Tag type='info' variant='small'>
                {t.common.adjectives.copied}!
              </Tag>
            </animated.div>
          </FeedbackContainer>
          <CopyIcon />
        </Button>
      </StyledBox>
    </>
  )
}

export default CopyBox
