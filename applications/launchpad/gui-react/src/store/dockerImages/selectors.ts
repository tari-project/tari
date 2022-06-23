import { RootState } from '../'

export const selectDockerImages = ({ dockerImages }: RootState) =>
  dockerImages.images

export const selectDockerImagesLoading = ({ dockerImages }: RootState) =>
  !dockerImages.loaded
