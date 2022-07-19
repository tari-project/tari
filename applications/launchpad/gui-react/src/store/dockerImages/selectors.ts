import { ContainerName } from '../../types/general'
import { RootState } from '../'

export const selectDockerImages = ({ dockerImages }: RootState) =>
  dockerImages.images

export const selectDockerImagesLoading = ({ dockerImages }: RootState) =>
  !dockerImages.loaded

export const selectRecipe =
  (container: ContainerName) =>
  ({ dockerImages }: RootState) =>
    dockerImages.recipes[container] || [container]

export const selectDockerTBotQueue = ({ dockerImages }: RootState) =>
  dockerImages.downloadWithTBot
