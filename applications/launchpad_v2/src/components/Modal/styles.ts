import styled from 'styled-components'

import type { ModalProps } from './types'

export const ModalContainer = styled.div`
  position: fixed;
  top: 0;
  bottom: 0;
  left: 0;
  right: 0;
  z-index: 9001;
  display: flex;
  justify-content: center;
  align-items: center;
`

export const Backdrop = styled.div`
  background: ${({ theme }) => theme.secondary};
  opacity: 0.1;
  position: absolute;
  top: 0;
  bottom: 0;
  right: 0;
  left: 0;
  z-index: 1;
`

export const ModalContent = styled.div<Pick<ModalProps, 'size'>>`
  width: ${({ size }) => (size === 'large' ? '880px' : '449px')};
  height: 642px;
  max-width: 80vw;
  max-height: 80vh;
  background: ${({ theme }) => theme.background};
  border-radius: ${({ theme }) => theme.borderRadius()};
  z-index: 2;
  box-sizing: border-box;
  box-shadow: ${({ theme }) => theme.shadow};
`
