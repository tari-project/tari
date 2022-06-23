import { DockerImage } from '../../types/general'

export type DockerImagesState = {
  loaded: boolean
  images: DockerImage[]
}
