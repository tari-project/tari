import styled, { useTheme } from 'styled-components'

import Loading from '../Loading'

const Overlay = styled.div`
  position: absolute;
  padding: ${({ theme }) => theme.spacing(2)};
  display: flex;
  justify-content: center;
  align-items: center;
  top: 0;
  bottom: 0;
  right: 0;
  left: 0;
  z-index: 1;
  backdrop-filter: grayscale(90%);
`

const LoadingOverlay = ({ inverted }: { inverted?: boolean }) => {
  const theme = useTheme()

  return (
    <Overlay>
      <Loading
        loading
        size='2em'
        color={inverted ? theme.inverted.primary : theme.primary}
        style={{
          position: 'sticky',
          top: theme.spacing(2),
          bottom: theme.spacing(2),
        }}
      />
    </Overlay>
  )
}

export default LoadingOverlay
