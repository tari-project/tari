import styled from 'styled-components'

import type { ModalProps } from './types'

export const ModalContainer = styled.div<Pick<ModalProps, 'local'>>`
  position: ${({ local }) => (local ? 'absolute' : 'fixed')};
  top: 0;
  bottom: 0;
  left: 0;
  right: 0;
  width: 100% !important;
  z-index: 100;
  display: flex;
  justify-content: center;
  align-items: center;
`

export const ModalContent = styled.div<Pick<ModalProps, 'size'>>`
  position: relative;
  width: ${({ size }) => {
    if (size === 'large') {
      return '880px'
    }

    if (size === 'small') {
      return '449px'
    }

    return 'auto'
  }};
  height: ${({ size }) => (size === 'auto' ? 'auto' : '642px')};
  max-width: 80vw;
  max-height: 80vh;
  background: ${({ theme }) => theme.nodeBackground};
  border-radius: ${({ theme }) => theme.borderRadius()};
  z-index: 2;
  box-sizing: border-box;
  box-shadow: ${({ theme }) => theme.shadow40};
`
