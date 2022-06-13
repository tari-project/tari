import { useSelector } from 'react-redux'

import Button from '../../../components/Button'
import t from '../../../locales'
import { selectCanAnyMiningNodeRun } from '../../../store/mining/selectors'
import SvgChart from '../../../styles/Icons/Chart'
import SvgClock from '../../../styles/Icons/Clock'
import SvgSetting2 from '../../../styles/Icons/Setting2'

/**
 * Renders set of links/actions in Mining dashboard
 */
const MiningViewActions = ({
  openScheduling,
  toggleStatistics,
  openSettings,
}: {
  openScheduling: () => void
  toggleStatistics: () => void
  openSettings: () => void
}) => {
  const canAnyMiningBeRun = useSelector(selectCanAnyMiningNodeRun)

  return (
    <div data-testid='mining-view-actions-cmp'>
      <Button
        autosizeIcons={false}
        variant='text'
        leftIcon={<SvgClock width='1.5rem' height='1.5rem' />}
        testId='mining-action-setup-mining-hours'
        disabled={!canAnyMiningBeRun}
        onClick={openScheduling}
        style={{ paddingLeft: 0 }}
      >
        {t.mining.viewActions.setUpMiningHours}
      </Button>
      <Button
        autosizeIcons={false}
        variant='text'
        leftIcon={<SvgSetting2 width='1.5rem' height='1.5rem' />}
        style={{ paddingLeft: 0 }}
        onClick={openSettings}
        testId='mining-view-actions-settings-btn'
      >
        {t.mining.viewActions.miningSettings}
      </Button>
      <Button
        autosizeIcons={false}
        variant='text'
        leftIcon={<SvgChart width='1.5rem' height='1.5rem' />}
        onClick={toggleStatistics}
        style={{ paddingLeft: 0 }}
      >
        {t.mining.viewActions.statistics}
      </Button>
    </div>
  )
}

export default MiningViewActions
