import LoadingIcon from '../../styles/Icons/Loading'

import { StyledSpan } from './styles'

/**
 * Loading
 * renders a spinning loading indicator
 *
 * @prop {boolean} loading - controls whether the indicator should be shown or not
 */
const Loading = ({ loading }: { loading?: boolean }) =>
  loading ? (
    <StyledSpan>
      <LoadingIcon />
    </StyledSpan>
  ) : null

export default Loading
