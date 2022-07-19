import * as React from 'react'
import { SVGProps } from 'react'

const SvgVolumeCross = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-volumecross'
    {...props}
  >
    <path
      d='M7.866 7.006h-.738a7.07 7.07 0 0 0-2.436.434c-1.367.502-2.362 1.753-2.59 3.258l-.008.05a8.366 8.366 0 0 0 0 2.504l.008.05c.228 1.505 1.223 2.756 2.59 3.258a7.07 7.07 0 0 0 2.436.434h.738c.445 0 .871.187 1.184.52l.541.577c1.597 1.7 4.347.896 4.88-1.428a20.854 20.854 0 0 0 0-9.326c-.533-2.324-3.283-3.129-4.88-1.428l-.541.577c-.313.333-.74.52-1.184.52Z'
      stroke='currentColor'
      strokeWidth={1.5}
    />
    <path
      d='m22 10-4 4m4 0-4-4'
      stroke='currentColor'
      strokeWidth={1.5}
      strokeLinecap='round'
    />
  </svg>
)

export default SvgVolumeCross
