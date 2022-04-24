import LoadingIcon from '../../styles/Icons/Loading'
import styled, { keyframes } from 'styled-components'

const spinKeyframes = keyframes`
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
`

const StyledSpan = styled.span`
  animation: ${spinKeyframes} infinite 2s linear;
`

const Loading = ({ loading }: { loading: boolean }) =>
  loading ? (
    <StyledSpan>
      <LoadingIcon />
    </StyledSpan>
  ) : null

export default Loading
