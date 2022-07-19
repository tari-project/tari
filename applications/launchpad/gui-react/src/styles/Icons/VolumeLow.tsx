import * as React from 'react'
import { SVGProps } from 'react'

const SvgVolumeLow = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-volumelow'
    {...props}
  >
    <path
      d='M20 10c.598.4 1 1.145 1 2s-.402 1.6-1 2'
      stroke='currentColor'
      strokeWidth={1.5}
      strokeLinecap='round'
      strokeLinejoin='round'
    />
    <path
      d='M9.317 7.006h-.794a8.14 8.14 0 0 0-2.624.434c-1.473.502-2.544 1.753-2.79 3.258l-.008.05a7.775 7.775 0 0 0 0 2.504l.009.05c.245 1.505 1.316 2.756 2.789 3.258.843.287 1.73.434 2.624.434h.794c.48 0 .938.187 1.275.52l.583.577c1.72 1.7 4.681.896 5.256-1.428a19.403 19.403 0 0 0 0-9.326c-.575-2.324-3.536-3.129-5.256-1.428l-.583.577c-.337.333-.796.52-1.275.52Z'
      stroke='currentColor'
      strokeWidth={1.5}
    />
  </svg>
)

export default SvgVolumeLow
