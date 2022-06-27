import { useTheme } from 'styled-components'

import Text from '../../../components/Text'
import Box from '../../../components/Box'
import DockerImagesList from '../../../components/DockerImagesList'

import { Wrapper, ScrollContainer } from './styles'

/**
 * Onboarding's Expert View
 */
const ExpertView = () => {
  const theme = useTheme()

  return (
    <Wrapper>
      <Text color={theme.inverted.primary}>Pulling images</Text>
      <ScrollContainer>
        <Box
          style={{
            backgroundColor: theme.inverted.backgroundSecondary,
            marginTop: 0,
          }}
          border={false}
        >
          <DockerImagesList inverted headers disableIcons />
        </Box>
      </ScrollContainer>
    </Wrapper>
  )
}

export default ExpertView
