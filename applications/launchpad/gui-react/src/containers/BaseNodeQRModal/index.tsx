import QRCode from 'react-qr-code'
import Button from '../../components/Button'

import Modal from '../../components/Modal'
import Text from '../../components/Text'

import t from '../../locales'
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
 * @TODO Replace with real data
 */
const QRContent =
  'tari://dibbler/base_nodes/add?name=00000000000000000000000000&peer=::/onion3/0000000000000000000000000000000000000000000000000000000000000000:00000'

/**
 * The modal rendering the Base Node address as QR code.
 * @param {boolean} open - show modal
 * @param {() => void} onClose - on modal close
 */
const BaseNodeQRModal = ({ open, onClose }: BaseNodeQRModalProps) => {
  return (
    <Modal open={open} onClose={onClose} size='small'>
      <ModalContainer>
        <Content>
          <Text as='h2' type='subheader'>
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
              value={QRContent}
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
