import { animated } from 'react-spring'
import styled from 'styled-components'

import { CSSShadowDefinition } from './types'

const SHADOW_SIZE_SCALE = 0.6

export const TBotContainer = styled(animated.div)<{
  shadow?: CSSShadowDefinition
}>`
  display: flex;
  align-items: center;
  justify-content: center;
  ${({ shadow }) =>
    shadow
      ? `margin: ${
          (shadow.spread +
            shadow.blur -
            shadow.size * (1 - SHADOW_SIZE_SCALE)) /
          2
        }px 0;`
      : ''}
`

export const TBotScaleContainer = styled(animated.div)`
  display: flex;
  align-items: center;
  justify-content: center;
  position: relative;
`

export const TBotShadow = styled.div<{ shadow: CSSShadowDefinition }>`
  position: absolute;
  width: ${({ shadow }) => shadow.size * SHADOW_SIZE_SCALE}px;
  height: ${({ shadow }) => shadow.size * SHADOW_SIZE_SCALE}px;
  box-shadow: ${({ shadow }) =>
    `0px 0px ${shadow.blur}px ${shadow.spread}px ${shadow.color}`};
  border-radius: 50%;
  z-index: 0;
`
