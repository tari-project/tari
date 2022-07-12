import styled from 'styled-components'

const Backdrop = styled.div<{ $opacity?: number; $borderRadius?: string }>`
  background: ${({ theme }) => theme.modalBackdrop};
  opacity: ${({ $opacity }) => $opacity};
  position: absolute;
  border-radius: ${({ $borderRadius }) => $borderRadius};
  top: 0;
  bottom: 0;
  right: 0;
  left: 0;
  z-index: 1;
`
Backdrop.defaultProps = {
  $opacity: 0.1,
  $borderRadius: '0',
}

export default Backdrop
