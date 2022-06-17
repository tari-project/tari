import { useTheme } from 'styled-components'

import Modal from '../../../components/Modal'
import Text from '../../../components/Text'
import Button from '../../../components/Button'
import TBot from '../../../components/TBot'
import CoinsList from '../../../components/CoinsList'

import { ContentWrapper, MessageWrapper } from './styles'

// eslint-disable-next-line quotes
const whatHaveYouJustDone = `You've just mined a Tari block!`

const TariNotificationContainer = ({
  amount,
  onClose,
}: {
  amount: number
  onClose: () => void
}) => {
  const theme = useTheme()

  return (
    <Modal open onClose={onClose} size='small'>
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
          <Text type='subheader'>{whatHaveYouJustDone}</Text>
          <TBot type='hearts' shadow />
          <Text style={{ textAlign: 'center' }}>
            Congratarilations! A new Tari block has been mined!
          </Text>
          <CoinsList coins={[{ amount, unit: 'xtr' }]} />
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

export default TariNotificationContainer
