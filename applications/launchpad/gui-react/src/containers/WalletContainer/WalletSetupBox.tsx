import { useTheme } from 'styled-components'

import SvgTariSignet from '../../styles/Icons/TariSignet'
import WalletPasswordWizard from '../../containers/WalletPasswordWizard'
import NodeBox from '../../components/NodeBox'
import t from '../../locales'

import WalletHelp from './WalletHelp'

const WalletSetupBox = () => {
  const theme = useTheme()

  return (
    <div style={{ display: 'flex', flexDirection: 'column' }}>
      <div
        style={{
          position: 'fixed',
          right: '10vw',
          bottom: '20vh',
          pointerEvents: 'none',
        }}
      >
        <SvgTariSignet
          color={theme.disabledPrimaryButton}
          width='auto'
          height='33vh'
        />
      </div>
      <NodeBox
        title={t.wallet.setUpTariWalletTitle}
        titleStyle={{ color: theme.helpTipText }}
        tag={{
          content: t.common.phrases.startHere,
        }}
        style={{
          position: 'relative',
          boxShadow: theme.shadow40,
          borderColor: theme.walletSetupBorderColor,
          background: theme.nodeBackground,
        }}
      >
        <WalletPasswordWizard
          submitBtnText={t.wallet.setUpTariWalletSubmitBtn}
        />
      </NodeBox>
      <WalletHelp />
    </div>
  )
}

export default WalletSetupBox
