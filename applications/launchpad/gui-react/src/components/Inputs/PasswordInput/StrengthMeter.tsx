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
  const [offset, setOffset] = useState<number | undefined>(undefined)
  const [strength, setStrength] = useState<number>(1)

  useEffect(() => {
    if (pathRef.current && pathRef.current.getTotalLength) {
      setOffset((pathRef.current as SVGCircleElement).getTotalLength())
    }
  }, [offset])

  useEffect(() => {
    if (!password) {
      setStrength(0)
    } else {
      const { score } = zxcvbn(password)
      setStrength((score + 1) / 5)
    }
  }, [password])

  const getColor = () => {
    if (strength >= 0.7) {
      return theme.success
    } else if (strength >= 0.4) {
      return theme.warningText
    }

    return theme.error
  }

  const { progress, color } = useSpring({
    progress:
      strength >= 1
        ? -Math.round(0.99 * (offset || 0))
        : -Math.round(strength * (offset || 0)),
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
        {strength === 0 ? (
          <animated.circle
            strokeDashoffset={offset}
            strokeDasharray={offset}
            strokeWidth='2'
            cx='11'
            cy='11'
            r='9'
            stroke='transparent'
            fill='none'
            ref={pathRef}
          />
        ) : (
          <animated.circle
            strokeDashoffset={progress}
            strokeDasharray={offset}
            strokeWidth='2'
            cx='11'
            cy='11'
            r='9'
            stroke={color}
            fill='none'
            ref={pathRef}
          />
        )}
      </svg>
    </StyledStrengthMeter>
  )
}

export default StrengthMeter
