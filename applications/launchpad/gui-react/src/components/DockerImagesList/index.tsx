import { useEffect } from 'react'
import { useTheme } from 'styled-components'

import { useAppSelector, useAppDispatch } from '../../store/hooks'
import { actions } from '../../store/dockerImages'
import {
  selectDockerImages,
  selectDockerImagesLoading,
} from '../../store/dockerImages/selectors'
import Text from '../../components/Text'
import Loading from '../../components/Loading'
import LoadingOverlay from '../../components/LoadingOverlay'
import Tag from '../../components/Tag'
import Button from '../../components/Button'
import CheckIcon from '../../styles/Icons/CheckRound'
import QuestionMarkIcon from '../../styles/Icons/Info1'
import t from '../../locales'

import { DockerRow, DockerList, DockerStatusWrapper } from './styles'

const DockerImagesList = () => {
  const theme = useTheme()
  const dispatch = useAppDispatch()
  useEffect(() => {
    dispatch(actions.getDockerImageList())
  }, [dispatch])

  const dockerImages = useAppSelector(selectDockerImages)
  const dockerImagesLoading = useAppSelector(selectDockerImagesLoading)

  return (
    <DockerList>
      {dockerImagesLoading && <LoadingOverlay />}
      {dockerImages.map(dockerImage => (
        <DockerRow key={dockerImage.dockerImage}>
          <Text style={{ flexBasis: '30%' }}>{dockerImage.displayName}</Text>
          {dockerImage.latest && (
            <DockerStatusWrapper>
              <CheckIcon
                color={theme.onTextLight}
                height='1.25em'
                width='1.25em'
                style={{
                  flexShrink: 0,
                  flexBasis: '2em',
                }}
              />
              <Text
                type='smallMedium'
                as='span'
                style={{
                  flexShrink: 1,
                  overflowX: 'hidden',
                  textOverflow: 'ellipsis',
                  wordBreak: 'keep-all',
                }}
              >
                {t.docker.imageUpToDate}{' '}
                <span
                  style={{ color: theme.secondary }}
                  title={dockerImage.dockerImage}
                >
                  {dockerImage.dockerImage}
                </span>
              </Text>
            </DockerStatusWrapper>
          )}
          {!dockerImage.latest && !dockerImage.pending && (
            <DockerStatusWrapper>
              <Tag type='warning'>{t.docker.newerVersion}</Tag>
              <Button
                variant='button-in-text'
                type='link'
                style={{ color: theme.onTextLight }}
                rightIcon={<QuestionMarkIcon />}
                onClick={() =>
                  dispatch(
                    actions.pullImage({ dockerImage: dockerImage.dockerImage }),
                  )
                }
              >
                {t.docker.pullImage}
              </Button>
            </DockerStatusWrapper>
          )}
          {!dockerImage.latest && dockerImage.pending && (
            <DockerStatusWrapper>
              <Loading loading size='1em' />
              {dockerImage.status && (
                <Text>{t.docker.status[dockerImage.status]}</Text>
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
