import { useState } from 'react'
import { useTheme } from 'styled-components'

import Text from '../../../components/Text'

import { useAppSelector } from '../../../store/hooks'
import { selectRecoveryPhraseCreated } from '../../../store/wallet/selectors'

import t from '../../../locales'

import { RowFlex, SettingsHeader, StyledList } from '../styles'
import SeedPhraseModal from '../../../components/SeedPhraseModal'
import Button from '../../../components/Button'
import Tag from '../../../components/Tag'
import SvgTick from '../../../styles/Icons/Tick'

const SecuritySettingsContainer = () => {
  const theme = useTheme()

  const alreadyCreated = useAppSelector(selectRecoveryPhraseCreated)

  const [openSeedPhraseModal, setOpenSeedPhraseModal] = useState(false)

  return (
    <>
      <SettingsHeader>
        <Text type='subheader' as='h2' color={theme.primary}>
          {t.settings.security.title}
        </Text>
      </SettingsHeader>

      <RowFlex
        style={{
          marginTop: theme.spacingVertical(1),
          marginBottom: theme.spacingVertical(1),
        }}
      >
        <Text type='smallMedium' color={theme.primary}>
          {t.settings.security.backupRecoveryPhrase}
        </Text>
        {alreadyCreated ? (
          <Tag variant='small' type='running' icon={<SvgTick />}>
            {t.common.adjectives.created}
          </Tag>
        ) : (
          <Tag variant='small' type='info'>
            {t.common.adjectives.recommended}
          </Tag>
        )}
      </RowFlex>

      <div
        style={{
          color: theme.secondary,
          marginBottom: theme.spacingVertical(1.5),
        }}
      >
        <Text type='smallMedium'>{t.settings.security.tab.desc}</Text>
        <StyledList>
          <li>
            <Text type='smallMedium'>{t.settings.security.tab.list1}</Text>
          </li>
          <li>
            <Text type='smallMedium'>{t.settings.security.tab.list2}</Text>
          </li>
          <li>
            <Text type='smallMedium'>{t.settings.security.tab.list3}</Text>
          </li>
        </StyledList>
      </div>

      <div style={{ display: 'inline-flex' }}>
        {alreadyCreated ? (
          <Button onClick={() => undefined} disabled>
            {t.settings.security.alreadyCreated}
          </Button>
        ) : (
          <Button onClick={() => setOpenSeedPhraseModal(true)}>
            {t.settings.security.createRecoveryPhrase}
          </Button>
        )}
      </div>
      <SeedPhraseModal
        open={openSeedPhraseModal}
        setOpen={() => setOpenSeedPhraseModal(false)}
      />
    </>
  )
}

export default SecuritySettingsContainer
