import Button from '../../../components/Button'
import Text from '../../../components/Text'

import { useAppDispatch } from '../../../store/hooks'
import { MergedMiningSetupRequired } from '../../../store/mining/types'
import { actions as settingsActions } from '../../../store/settings'

import t from '../../../locales'

import { SetupMergedContent } from './styles'

const SetupMerged = ({
  mergedSetupRequired,
}: {
  mergedSetupRequired: MergedMiningSetupRequired
}) => {
  const dispatch = useAppDispatch()

  return (
    <SetupMergedContent>
      <Text style={{ maxWidth: 242 }}>
        {t.mining.setup.description}{' '}
        <Text as='span' type='defaultHeavy'>
          {t.mining.setup.descriptionBold}
        </Text>
      </Text>
      <Button
        variant='primary'
        onClick={() => dispatch(settingsActions.open({}))}
        disabled={
          mergedSetupRequired === MergedMiningSetupRequired.MissingWalletAddress
        }
        testId='setup-merged-open-settings'
      >
        {t.mining.actions.setupAndStartMining}
      </Button>
    </SetupMergedContent>
  )
}

export default SetupMerged
