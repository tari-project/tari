import t from '../../locales'
import Loading from '../Loading'
import Tag from '../Tag'

import { StyledTabContent, TabMainText, LoadingWrapper } from './styles'

const TabContent = ({
  text,
  running,
  pending,
  tagSubText,
}: {
  text: string
  running?: boolean
  pending?: boolean
  tagSubText?: string
}) => {
  return (
    <StyledTabContent>
      <TabMainText>{text}</TabMainText>
      {running && !pending ? (
        <Tag variant='small' type='running' subText={tagSubText} dark>
          {t.common.adjectives.running}
        </Tag>
      ) : null}
      {pending ? (
        <LoadingWrapper>
          <Loading loading={true} size='12px' />
        </LoadingWrapper>
      ) : null}
    </StyledTabContent>
  )
}

export default TabContent
