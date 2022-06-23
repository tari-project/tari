import { useTheme } from 'styled-components'

import { useAppSelector, useAppDispatch } from '../../../../store/hooks'
import { pullImage } from '../../../../store/app'
import {
  selectDockerImages,
  selectDockerImagesLoading,
} from '../../../../store/app/selectors'
import Text from '../../../../components/Text'
import Tag from '../../../../components/Tag'
import Button from '../../../../components/Button'
import CheckIcon from '../../../../styles/Icons/CheckRound'
import QuestionMarkIcon from '../../../../styles/Icons/Info1'
import t from '../../../../locales'

import { DockerRow, DockerList, DockerStatusWrapper } from './styles'

const DockerImagesList = () => {
  const theme = useTheme()
  const dispatch = useAppDispatch()
  const dockerImages = useAppSelector(selectDockerImages)
  const dockerImagesLoading = useAppSelector(selectDockerImagesLoading)

  return (
    <DockerList>
      {dockerImagesLoading && <p>LOADING</p>}
      {dockerImages.map(dockerImage => (
        <DockerRow key={dockerImage.dockerImage}>
          <Text style={{ flexBasis: '40%' }}>{dockerImage.displayName}</Text>
          {dockerImage.latest && (
            <CheckIcon color={theme.onTextLight} height='1.25em' width='auto' />
          )}
          {!dockerImage.latest && !dockerImage.pending && (
            <DockerStatusWrapper>
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
            </DockerStatusWrapper>
          )}
          {!dockerImage.latest && dockerImage.pending && (
            <DockerStatusWrapper>
              {dockerImage.status && (
                <Text>{t.docker.settings.status[dockerImage.status]}</Text>
              )}
              {dockerImage.progress !== undefined && (
                <Text>{dockerImage.progress}%</Text>
              )}
            </DockerStatusWrapper>
          )}
        </DockerRow>
      ))}
    </DockerList>
  )
}

export default DockerImagesList
