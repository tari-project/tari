import { useTheme } from 'styled-components'

import { BlockMinedNotification } from '../../../store/mining/types'
import Modal from '../../../components/Modal'
import Text from '../../../components/Text'
import Button from '../../../components/Button'
import TBot from '../../../components/TBot'
import CoinsList from '../../../components/CoinsList'

import { ContentWrapper, MessageWrapper } from './styles'

const TariNotificationComponent = ({
  notification,
  open,
  onClose,
}: {
  notification: BlockMinedNotification
  open: boolean
  onClose: () => void
}) => {
  const theme = useTheme()

  return (
    <Modal open={open} onClose={onClose} size='small'>
      <ContentWrapper>
        <MessageWrapper>
          <div>
            <Text as='span' type='subheader'>
              Fan
            </Text>
            <Text as='span' type='subheader' color={theme.accent}>
              tari
            </Text>
            <Text as='span' type='subheader'>
              tastic
            </Text>
          </div>
          <Text type='subheader'>{notification.header}</Text>
          <TBot type='hearts' shadow animate={false} />
          <Text style={{ textAlign: 'center' }}>{notification.message}</Text>
          <CoinsList
            coins={[
              { amount: notification.amount, unit: notification.currency },
            ]}
          />
          <Text>has been added to your wallet.</Text>
        </MessageWrapper>
        <Button
          style={{ width: '100%', justifyContent: 'center' }}
          onClick={onClose}
        >
          Got it
        </Button>
      </ContentWrapper>
    </Modal>
  )
}

export default TariNotificationComponent
