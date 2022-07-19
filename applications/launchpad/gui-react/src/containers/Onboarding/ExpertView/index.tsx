import { useTheme } from 'styled-components'

import Text from '../../../components/Text'
import Box from '../../../components/Box'
import DockerImagesList from '../../../components/DockerImagesList'
import t from '../../../locales'

import { Wrapper, ScrollContainer } from './styles'

/**
 * Onboarding's Expert View
 */
const ExpertView = () => {
  const theme = useTheme()

  return (
    <Wrapper>
      <Text color={theme.inverted.primary}>
        {t.onboarding.expertView.title}
      </Text>
      <ScrollContainer>
        <Box
          style={{
            backgroundColor: theme.inverted.backgroundSecondary,
            marginTop: 0,
            width: '100%',
          }}
          border={false}
        >
          <DockerImagesList inverted header disableIcons />
        </Box>
      </ScrollContainer>
    </Wrapper>
  )
}

export default ExpertView
