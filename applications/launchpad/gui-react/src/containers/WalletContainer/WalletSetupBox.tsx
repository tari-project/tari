import { useTheme } from 'styled-components'

import SvgTariSignet from '../../styles/Icons/TariSignet'
import WalletPasswordWizard from '../../containers/WalletPasswordWizard'
import NodeBox from '../../components/NodeBox'
import t from '../../locales'

const WalletSetupBox = () => {
  const theme = useTheme()

  return (
    <>
      <div style={{ position: 'fixed', right: '10vw', bottom: '20vh' }}>
        <SvgTariSignet
          color={theme.backgroundImage}
          width='auto'
          height='33vh'
        />
      </div>
      <NodeBox
        title={t.wallet.setUpTariWalletTitle}
        tag={{
          content: t.common.phrases.startHere,
        }}
        style={{
          position: 'relative',
          boxShadow: theme.shadow40,
          borderColor: 'transparent',
        }}
      >
        <WalletPasswordWizard
          submitBtnText={t.wallet.setUpTariWalletSubmitBtn}
        />
      </NodeBox>
    </>
  )
}

export default WalletSetupBox
