import { useTheme } from 'styled-components'

import { useAppSelector, useAppDispatch } from '../../../../store/hooks'
import { pullImage } from '../../../../store/app'
import { selectDockerImages } from '../../../../store/app/selectors'
import Text from '../../../../components/Text'
import Tag from '../../../../components/Tag'
import Button from '../../../../components/Button'
import CheckIcon from '../../../../styles/Icons/CheckRound'
import QuestionMarkIcon from '../../../../styles/Icons/Info1'
import t from '../../../../locales'

import { DockerRow } from './styles'

const DockerImagesList = () => {
  const theme = useTheme()
  const dispatch = useAppDispatch()
  const dockerImages = useAppSelector(selectDockerImages)

  return (
    <>
      {dockerImages.map(dockerImage => (
        <DockerRow key={dockerImage.dockerImage}>
          <Text style={{ flexBasis: '40%' }}>{dockerImage.displayName}</Text>
          {dockerImage.latest && (
            <CheckIcon color={theme.onTextLight} height='1.25em' width='auto' />
          )}
          {!dockerImage.latest && (
            <div
              style={{
                display: 'flex',
                columnGap: theme.spacingHorizontal(0.5),
              }}
            >
              <Tag type='warning'>{t.docker.settings.newerVersion}</Tag>
              <Button
                variant='button-in-text'
                type='link'
                style={{ color: theme.onTextLight }}
                rightIcon={<QuestionMarkIcon />}
                onClick={() =>
                  dispatch(pullImage({ dockerImage: dockerImage.dockerImage }))
                }
              >
                {t.docker.settings.pullImage}
              </Button>
            </div>
          )}
        </DockerRow>
      ))}
    </>
  )
}

export default DockerImagesList
