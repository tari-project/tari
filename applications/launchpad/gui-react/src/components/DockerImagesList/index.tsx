import { useEffect, CSSProperties, useState } from 'react'
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
import t from '../../locales'

import {
  DockerRow,
  DockerList,
  DockerStatusWrapper,
  ErrorWrapper,
  TextProgessContainer,
  ProgressContainer,
} from './styles'
import Alert from '../Alert'

const DockerImagesList = ({
  inverted,
  header,
  disableIcons,
  style,
}: {
  inverted?: boolean
  header?: boolean
  disableIcons?: boolean
  style?: CSSProperties
}) => {
  const theme = useTheme()
  const dispatch = useAppDispatch()
  useEffect(() => {
    dispatch(actions.getDockerImageList())
  }, [dispatch])

  const [errorInAlert, setErrorInAlert] = useState<string | undefined>(
    undefined,
  )

  const dockerImages = useAppSelector(selectDockerImages)
  const dockerImagesLoading = useAppSelector(selectDockerImagesLoading)

  return (
    <DockerList style={style}>
      {dockerImagesLoading && <LoadingOverlay inverted={inverted} />}
      {header && (
        <DockerRow key='headers'>
          <Text
            style={{ flexBasis: '30%' }}
            type='smallMedium'
            color={theme.inverted.secondary}
          >
            {t.docker.header.image}
          </Text>
          <Text type='smallMedium' color={theme.inverted.secondary}>
            {t.docker.header.status}
          </Text>
        </DockerRow>
      )}
      {dockerImages.map(dockerImage => {
        return (
          <DockerRow key={dockerImage.dockerImage} $inverted={inverted}>
            <Text
              style={{ flexBasis: '30%' }}
              type={header ? 'smallMedium' : 'defaultMedium'}
              color={inverted ? theme.inverted.disabledText : theme.primary}
            >
              {dockerImage?.displayName?.toLowerCase() || ''}
            </Text>
            {dockerImage.updated && (
              <DockerStatusWrapper>
                {!disableIcons && (
                  <CheckIcon
                    color={theme.onTextLight}
                    height='1.25em'
                    width='1.25em'
                    style={{
                      flexShrink: 0,
                      flexBasis: '2em',
                    }}
                  />
                )}
                <Text
                  type='smallMedium'
                  as='span'
                  style={{
                    flexShrink: 1,
                    overflowX: 'hidden',
                    textOverflow: 'ellipsis',
                    wordBreak: 'keep-all',
                  }}
                  color={inverted ? theme.inverted.secondary : theme.primary}
                >
                  {t.docker.imageUpToDate}{' '}
                  <span
                    style={{
                      color: inverted
                        ? theme.inverted.primary
                        : theme.secondary,
                    }}
                    title={dockerImage.dockerImage}
                  >
                    {dockerImage.dockerImage}
                  </span>
                </Text>
              </DockerStatusWrapper>
            )}
            {!dockerImage.updated && !dockerImage.pending && (
              <DockerStatusWrapper>
                {dockerImage.error ? (
                  <ErrorWrapper
                    onClick={() => setErrorInAlert(dockerImage.error)}
                  >
                    <Text
                      as='span'
                      style={{ fontSize: 12, color: theme.error }}
                    >
                      {dockerImage.error}
                    </Text>
                  </ErrorWrapper>
                ) : (
                  <Tag type='warning'>{t.docker.newerVersion}</Tag>
                )}
                <Button
                  variant='button-in-text'
                  type='link'
                  style={{ color: theme.onTextLight }}
                  onClick={() =>
                    dispatch(
                      actions.pullImage({
                        dockerImage: dockerImage.containerName,
                      }),
                    )
                  }
                >
                  {t.docker.pullImage}
                </Button>
              </DockerStatusWrapper>
            )}
            {!dockerImage.updated && dockerImage.pending && (
              <DockerStatusWrapper>
                <Loading
                  loading
                  size='1em'
                  color={inverted ? theme.inverted.primary : theme.primary}
                />
                <ProgressContainer>
                  {dockerImage.status && (
                    <TextProgessContainer>
                      <Text
                        color={
                          inverted ? theme.inverted.primary : theme.secondary
                        }
                        style={{ fontSize: 12 }}
                      >
                        {dockerImage.status}
                      </Text>
                    </TextProgessContainer>
                  )}
                  {dockerImage.progress !== undefined && (
                    <TextProgessContainer>
                      <Text
                        color={
                          inverted ? theme.inverted.primary : theme.secondary
                        }
                        style={{
                          fontSize: '9px',
                          whiteSpace: 'pre-line',
                          lineHeight: '10px',
                        }}
                      >
                        {dockerImage.progress?.split(']').join(']\n')}
                      </Text>
                    </TextProgessContainer>
                  )}
                </ProgressContainer>
              </DockerStatusWrapper>
            )}
          </DockerRow>
        )
      })}
      <Alert
        title='Error'
        open={Boolean(errorInAlert)}
        onClose={() => setErrorInAlert(undefined)}
        content={errorInAlert}
      />
    </DockerList>
  )
}

export default DockerImagesList
