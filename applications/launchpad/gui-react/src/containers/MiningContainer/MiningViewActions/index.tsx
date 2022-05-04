import Button from '../../../components/Button'
import t from '../../../locales'
import SvgChart from '../../../styles/Icons/Chart'
import SvgClock from '../../../styles/Icons/Clock'
import SvgSetting2 from '../../../styles/Icons/Setting2'

/**
 * Renders set of links/actions in Mining dashboard
 */
const MiningViewActions = () => {
  return (
    <div>
      <Button variant='text' leftIcon={<SvgClock />}>
        {t.mining.viewActions.setUpMiningHours}
      </Button>
      <Button variant='text' leftIcon={<SvgSetting2 />}>
        {t.mining.viewActions.miningSettings}
      </Button>
      <Button variant='text' leftIcon={<SvgChart />}>
        {t.mining.viewActions.statistics}
      </Button>
    </div>
  )
}

export default MiningViewActions
