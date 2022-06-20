import { BlockMinedNotification } from '../../../store/mining/types'
import Modal from '../../../components/Modal'
import Text from '../../../components/Text'
import Button from '../../../components/Button'
import TBot from '../../../components/TBot'
import CoinsList from '../../../components/CoinsList'
import t from '../../../locales'

import TariText from './TariText'
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
  return (
    <Modal open={open} onClose={onClose} size='small'>
      <ContentWrapper>
        <MessageWrapper>
          <TariText>{notification.header}</TariText>
          <TBot type='hearts' shadow disableEnterAnimation />
          <TariText style={{ textAlign: 'center' }} type='defaultMedium'>
            {notification.message}
          </TariText>
          <CoinsList
            coins={[
              { amount: notification.amount, unit: notification.currency },
            ]}
          />
          <Text>{t.mining.notification.added}</Text>
        </MessageWrapper>
        <Button
          style={{ width: '100%', justifyContent: 'center' }}
          onClick={onClose}
        >
          {t.mining.notification.ack}
        </Button>
      </ContentWrapper>
    </Modal>
  )
}

export default TariNotificationComponent
