import styled from 'styled-components'
import { animated } from 'react-spring'

export const PromptContainer = styled(animated.div)<{ $floating?: boolean }>`
  position: ${({ $floating }) => ($floating ? 'fixed' : 'static')};
  right: 40px;
  bottom: 40px;
  z-index: 1;
  width: ${({ $floating }) => ($floating ? '476px' : '692px')};
  height: fit-content;
  display: flex;
  flex-direction: column;
  justify-content: center;
`

export const ContentRow = styled(animated.div)<{ $floating?: boolean }>`
  width: ${({ $floating }) => ($floating ? '417px' : '628px')};
  display: flex;
  flex-direction: row;
  justify-content: flex-start;
`

export const ContentContainer = styled(animated.div)<{ $floating?: boolean }>`
  display: flex;
  justify-content: center;
  height: fit-content;
  margin-right: 30px;
  border-radius: ${({ theme }) => theme.borderRadius(2)};
  /* hard-code required here */
  ${({ $floating }) => ($floating ? 'background-color: #20053d05;' : '')}
  backdrop-filter: blur(9px);
  padding-bottom: 12px;
`

/**
 * @TODO: - wrong behaviour in non-$floating variant, open issue https://github.com/Altalogy/tari/issues/221
 */
export const FadeOutSection = styled.div<{ $floating?: boolean }>`
  position: absolute;
  height: ${({ $floating }) => ($floating ? '100px' : '250px')};
  ${({ $floating }) => ($floating ? '' : 'top: 0;')}
  width: ${({ $floating }) => ($floating ? '417px' : '628px')};
  z-index: 2;
  border-radius: ${({ theme }) => theme.borderRadius(2)};
  background-image: ${({ $floating }) =>
    $floating
      ? 'linear-gradient(to bottom, rgba(255, 255, 255, 1), rgba(255, 255, 255, 0.4))'
      : 'linear-gradient(to bottom, rgba(250, 250, 250, 1), rgba(250, 250, 250, 0.4))'};
`

export const MessageContainer = styled(animated.div)<{ $floating?: boolean }>`
  padding-left: ${({ $floating }) => ($floating ? '0px' : '10px')};
  padding-right: ${({ $floating }) => ($floating ? '0px' : '10px')};
`

export const ScrollWrapper = styled.div`
  max-height: calc(90vh - 250px);
  min-height: 50px;
  overflow-y: scroll;
  transition: max-height 200ms;
  z-index: 1;
  position: relative;
  padding-bottom: 20px;
  padding-top: 20px;

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

export const MessageWrapper = styled.div`
  padding-top: 20px;
`
export const HeightAnimationWrapper = styled(animated.div)`
  max-height: 200px;
  overflow: hidden;
  min-height: 30px;
`

export const TBotContainer = styled(animated.div)`
  display: flex;
  justify-content: flex-end;
`

export const StyledCloseContainer = styled.div`
  display: flex;
  justify-content: flex-end;
  align-items: center;
  height: 72px;
`

export const StyledCloseIcon = styled.div`
  color: ${({ theme }) => theme.secondary};
  cursor: pointer;
  margin-right: 27px;
  position: absolute;
  right: 59px;
  top: 24px;
  z-index: 3;
`

export const StyledMessageBox = styled.div`
  position: relative;
`

export const StyledMessage = styled(animated.div)<{
  $floating?: boolean
  skipButton?: boolean
}>`
  display: flex;
  flex-direction: column;
  width: ${({ $floating }) => ($floating ? '307px' : '550px')};
  height: fit-content;
  margin-bottom: ${({ theme, skipButton }) =>
    skipButton ? theme.spacingVertical(5) : theme.spacingVertical(0.6)};
  background-color: ${({ theme }) => theme.background};
  border-radius: ${({ theme }) => theme.borderRadius(2)};
  box-shadow: ${({ theme }) => theme.shadow24};
  padding: 40px;
  color: ${({ theme }) => theme.primary};
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
  overflow: hidden;
`

export const MessageSlideIn = styled(animated.div)`
  position: absolute;
  left: 0;
  right: 0;
`

export const SkipButtonContainer = styled.div`
  position: relative;
  width: 130px;
  margin-top: ${({ theme }) => theme.spacingHorizontal(0.5)};
`

export const TBotProgressContainer = styled.div<{ mode?: string }>`
  display: flex;
  width: 100%;
  justify-content: ${({ mode }) =>
    mode === 'onboarding' ? 'space-between' : 'flex-end'};
`
