import { CSSProperties, useState, useEffect, useRef } from 'react'
import { clipboard } from '@tauri-apps/api'
import { useSpring, animated } from 'react-spring'

import Button from '../Button'
import Text from '../Text'
import Tag from '../Tag'
import CopyIcon from '../../styles/Icons/Copy'
import t from '../../locales'

import { ValueContainer, StyledBox, FeedbackContainer } from './styles'

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
  labelColor,
  value,
  style,
  valueTransform,
}: {
  label?: string
  labelColor?: string
  value: string
  style?: CSSProperties
  valueTransform?: (s: string) => string
}) => {
  const [copied, setCopied] = useState(false)
  const styles = useSpring({ opacity: copied ? 1 : 0 })
  const timeout = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)

  const copy = async () => {
    const transformed = valueTransform ? valueTransform(value) : value
    await clipboard.writeText(transformed)

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
      {label && <Text color={labelColor}>{label}</Text>}
      <StyledBox style={style}>
        <ValueContainer title={value}>{value}</ValueContainer>
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
