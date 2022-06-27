import { useEffect, useRef, useState } from 'react'
import { useSpring, animated } from 'react-spring'
import { useTheme } from 'styled-components'
import zxcvbn from 'zxcvbn'
import { StyledStrengthMeter } from './styles'

/**
 * Calculate the password strength with zxcvbn and render circle meter.
 * @param {string} [password]
 */
const StrengthMeter = ({ password }: { password?: string }) => {
  const theme = useTheme()
  const pathRef = useRef<SVGCircleElement>(null)
  const [offset, setOffset] = useState<number>(0)
  const [strength, setStrength] = useState<number>(1)

  useEffect(() => {
    if (pathRef.current?.getTotalLength) {
      setOffset((pathRef.current as SVGCircleElement).getTotalLength())
    }
  }, [])

  useEffect(() => {
    if (!password) {
      setStrength(0)
    } else {
      const { score } = zxcvbn(password)
      setStrength((score + 1) / 4)
    }
  }, [password])

  const getColor = () => {
    if (strength <= 0.25) {
      return theme.moneroDark
    } else if (strength > 0.25 && strength <= 0.5) {
      return theme.warningText
    } else if (strength > 0.5 && strength <= 0.75) {
      return theme.infoText
    } else {
      return theme.onTextLight
    }
  }

  const { progress, color } = useSpring({
    progress:
      strength >= 1
        ? `${Math.round(0.99 * offset)} ${offset - Math.round(0.99 * offset)}`
        : `${Math.round(strength * offset)} ${
            offset - Math.round(strength * offset)
          }`,
    color: getColor(),
    tension: 4,
    friction: 0.5,
    precision: 0.1,
  })

  return (
    <StyledStrengthMeter>
      <svg
        viewBox='0 0 22 22'
        height='22px'
        width='22px'
        data-testid='strength-meter'
        data-strength={strength}
      >
        <animated.circle
          strokeDasharray={progress}
          strokeWidth='2'
          cx='11'
          cy='11'
          r='9'
          stroke={strength === 0 ? 'transparent' : color}
          fill='none'
          ref={pathRef}
        />
      </svg>
    </StyledStrengthMeter>
  )
}

export default StrengthMeter
