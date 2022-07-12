import styled from 'styled-components'
import { animated } from 'react-spring'
import { TITLE_BAR_HEIGHT } from '../../TitleBar/styles'
import colors from '../../../styles/styles/colors'

export const PROMPT_HEIGHT_SPACING = 250
export const CLOSE_BTN_HEIGHT = 72
export const TBOT_CONTAINER_TOP_PADDING = 20

export const TBotContainerSizes = {
  sm: {
    containerWidth: 476,
    messageWidth: 426,
    fadeOutHeight: 80,
  },
  md: {
    containerWidth: 692,
    messageWidth: 622,
    fadeOutHeight: 220,
  },
}

const addPx = (val: number) => {
  return `${val}px`
}

export const PromptContainer = styled(animated.div)<{ $floating?: boolean }>`
  position: ${({ $floating }) => ($floating ? 'fixed' : 'static')};
  right: 40px;
  bottom: 40px;
  z-index: ${({ $floating }) => ($floating ? '200' : '1')};
  width: ${({ $floating }) =>
    $floating
      ? addPx(TBotContainerSizes.sm.containerWidth)
      : addPx(TBotContainerSizes.md.containerWidth)};
  max-width: 100%;
  height: ${({ $floating }) =>
    $floating ? 'fit-content' : `calc(100vh - ${PROMPT_HEIGHT_SPACING}px)`};
  max-width: 100%;
  display: flex;
  flex-direction: column;
  justify-content: ${({ $floating }) => ($floating ? 'center' : 'flex-end')};
`

export const ContentRow = styled(animated.div)<{ $floating?: boolean }>`
  position: relative;
  width: ${({ $floating }) =>
    $floating
      ? addPx(TBotContainerSizes.sm.containerWidth)
      : addPx(TBotContainerSizes.md.containerWidth)};
  max-width: 100%;
  ${({ $floating }) => ($floating ? '' : 'height: 100%;')}
  display: flex;
  flex-direction: row;
  justify-content: flex-start;
  align-items: flex-end;
  background-blend-mode: screen;
  height: fit-content;
`

export const ContentContainer = styled(animated.div)<{ $floating?: boolean }>`
  display: flex;
  position: relative;
  justify-content: center;
  height: fit-content;
  max-width: 100%;
  margin-right: 30px;
  border-radius: ${({ theme }) => theme.borderRadius(2)};
  background-color: ${({ $floating, theme }) =>
    $floating ? theme.tbotContentBackground : ''};
  backdrop-filter: blur(9px);
  padding-bottom: 12px;
  overflow: hidden;
  padding-top: ${CLOSE_BTN_HEIGHT}px;
`

export const FadeOutSection = styled(animated.div)<{
  $floating?: boolean
  $onDarkBg?: boolean
}>`
  pointer-events: none;
  position: absolute;
  height: ${({ $floating }) =>
    $floating
      ? addPx(TBotContainerSizes.sm.fadeOutHeight)
      : addPx(TBotContainerSizes.md.fadeOutHeight)};
  width: ${({ $floating }) =>
    $floating
      ? addPx(TBotContainerSizes.sm.containerWidth - 12)
      : addPx(TBotContainerSizes.md.containerWidth)};
  max-width: 100%;
  top: ${({ $floating }) =>
    $floating
      ? `${CLOSE_BTN_HEIGHT - 1}px`
      : `${
          TITLE_BAR_HEIGHT + CLOSE_BTN_HEIGHT - TBOT_CONTAINER_TOP_PADDING - 50
        }px`};
  left: 0;
  z-index: 2;
  background-image: ${({ $onDarkBg, $floating }) => {
    const bgBase = $onDarkBg ? '0, 0, 0' : '250, 250, 250'
    const firstStop = $floating ? '10%' : '20%'

    return `linear-gradient(to bottom, rgba(${bgBase}, 1) ${firstStop}, rgba(${bgBase}, 0) 100%)`
  }};
`

export const MessageContainer = styled(animated.div)<{ $floating?: boolean }>`
  padding-left: ${({ $floating }) => ($floating ? '0px' : '10px')};
  padding-right: ${({ $floating }) => ($floating ? '0px' : '10px')};
  max-width: 100%;
  width: 100%;
  box-sizing: border-box;
`

export const ScrollWrapper = styled.div`
  max-height: calc(90vh - ${PROMPT_HEIGHT_SPACING}px);
  min-height: 50px;
  max-width: 100%;
  overflow-y: scroll;
  transition: max-height 200ms;
  z-index: 1;
  position: relative;
  padding-bottom: 20px;
  padding-top: ${TBOT_CONTAINER_TOP_PADDING}px;
  padding-right: 8px;

  ::-webkit-scrollbar {
    width: 4px;
  }

  /* Track */
  ::-webkit-scrollbar-track {
    background: transparent;
  }

  /* Handle */
  ::-webkit-scrollbar-thumb {
    background: ${({ theme }) => theme.background};
    border-radius: 3px;
  }

  /* Handle on hover */
  ::-webkit-scrollbar-thumb:hover {
    background: #555;
  }
`

export const MessageWrapper = styled.div``

export const HeightAnimationWrapper = styled(animated.div)`
  max-height: 200px;
  min-height: 30px;
`

export const TBotContainer = styled(animated.div)`
  display: flex;
  justify-content: center;
  width: 80px;
`

export const StyledCloseContainer = styled.div`
  display: flex;
  justify-content: flex-end;
  align-items: center;
  height: ${CLOSE_BTN_HEIGHT}px;
  top: 0;
  position: absolute;
  right: 48px;
  z-index: 3;
`

export const StyledCloseIcon = styled.div`
  color: ${({ theme }) => theme.nodeWarningText};
  cursor: pointer;
`

export const StyledMessageBox = styled.div`
  position: relative;
`

export const StyledMessage = styled(animated.div)<{
  $floating?: boolean
  $skipButton?: boolean
  $onDarkBg?: boolean
}>`
  display: flex;
  flex-direction: column;
  width: ${({ $floating }) =>
    $floating
      ? addPx(TBotContainerSizes.sm.messageWidth)
      : addPx(TBotContainerSizes.md.messageWidth)};
  max-width: 100%;
  box-sizing: border-box;
  height: fit-content;
  margin-bottom: ${({ theme, $skipButton }) =>
    $skipButton ? theme.spacingVertical(5) : theme.spacingVertical(0.6)};
  background-color: ${({ theme, $onDarkBg }) =>
    $onDarkBg ? colors.darkMode.message : theme.tbotMessage};
  border-radius: ${({ theme }) => theme.borderRadius(2)};
  box-shadow: ${({ theme }) => theme.shadow24};
  padding: 40px;
  color: ${({ theme, $onDarkBg }) =>
    $onDarkBg ? colors.light.primary : theme.primary};
  &:last-child {
    margin-bottom: 0;
  }
`

export const MessageSpaceContainer = styled.div`
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  position: absolute;
`

export const MessageSlideIn = styled(animated.div)`
  position: absolute;
  left: 0;
  right: 0;
`

export const SkipButtonContainer = styled.div`
  margin-top: ${({ theme }) => theme.spacingVertical(2)};
`

export const TBotProgressContainer = styled.div<{ mode?: string }>`
  display: flex;
  width: 100%;
  justify-content: ${({ mode }) =>
    mode === 'onboarding' ? 'space-between' : 'flex-end'};
  height: 80px;
`
