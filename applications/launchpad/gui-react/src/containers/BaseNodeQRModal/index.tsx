import { useEffect, useState } from 'react'
import QRCode from 'react-qr-code'
import { useTheme } from 'styled-components'
import Button from '../../components/Button'

import Modal from '../../components/Modal'
import Text from '../../components/Text'

import t from '../../locales'
import {
  selectBaseNodeIdentity,
  selectNetwork,
  selectRunning,
} from '../../store/baseNode/selectors'
import { useAppDispatch, useAppSelector } from '../../store/hooks'
import { actions as baseNodeActions } from '../../store/baseNode'
import {
  ModalContainer,
  Content,
  CtaButton,
  Steps,
  Instructions,
  QRContainer,
} from './styles'

import { BaseNodeQRModalProps } from './types'

/**
 * The modal rendering the Base Node address as QR code.
 * @param {boolean} open - show modal
 * @param {() => void} onClose - on modal close
 */
const BaseNodeQRModal = ({ open, onClose }: BaseNodeQRModalProps) => {
  const theme = useTheme()
  const network = useAppSelector(selectNetwork)
  const baseNodeIdentity = useAppSelector(selectBaseNodeIdentity)
  const isBaseNodeRunning = useAppSelector(selectRunning)

  const dispatch = useAppDispatch()

  const [qrUrl, setQrUrl] = useState('')

  useEffect(() => {
    if (baseNodeIdentity) {
      setQrUrl(
        `tari://${network}/base_nodes/add?name=${baseNodeIdentity.nodeId}&peer=${baseNodeIdentity.publicKey}::${baseNodeIdentity.publicAddress}`,
      )
    }
  }, [baseNodeIdentity, network])

  useEffect(() => {
    if (isBaseNodeRunning && open) {
      dispatch(baseNodeActions.getBaseNodeIdentity())
    }
  }, [isBaseNodeRunning, open])

  return (
    <Modal open={open} onClose={onClose} size='small'>
      <ModalContainer>
        <Content>
          <Text as='h2' type='subheader' color={theme.primary}>
            {t.baseNode.qrModal.heading}
          </Text>
          <Instructions>
            <Text type='smallMedium'>{t.baseNode.qrModal.description}</Text>
            <Steps>
              <li>
                <Text as='span' type='smallMedium'>
                  {t.baseNode.qrModal.step1}
                </Text>
              </li>
              <li>
                <Text as='span' type='smallMedium'>
                  {t.baseNode.qrModal.step2}
                </Text>
              </li>
              <li>
                <Text as='span' type='smallMedium'>
                  {t.baseNode.qrModal.step3}
                </Text>
              </li>
              <li>
                <Text as='span' type='smallMedium'>
                  {t.baseNode.qrModal.step4}
                </Text>
              </li>
            </Steps>
          </Instructions>

          <QRContainer>
            <QRCode
              value={qrUrl}
              level='H'
              size={220}
              data-testid='base-node-qr-code'
            />
          </QRContainer>
        </Content>
        <CtaButton>
          <Button onClick={onClose} fullWidth>
            {t.baseNode.qrModal.submitBtn}
          </Button>
        </CtaButton>
      </ModalContainer>
    </Modal>
  )
}

export default BaseNodeQRModal
