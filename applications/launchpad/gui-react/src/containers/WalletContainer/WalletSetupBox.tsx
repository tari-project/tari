import { useTheme } from 'styled-components'

import WalletPasswordWizard from '../../containers/WalletPasswordWizard'
import NodeBox from '../../components/NodeBox'
import t from '../../locales'

const WalletSetupBox = () => {
  const theme = useTheme()

  return (
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
      <WalletPasswordWizard submitBtnText={t.wallet.setUpTariWalletSubmitBtn} />
    </NodeBox>
  )
}

export default WalletSetupBox
