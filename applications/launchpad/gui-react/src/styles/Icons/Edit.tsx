import * as React from 'react'
import { SVGProps } from 'react'

const SvgEdit = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-edit'
    {...props}
  >
    <path
      d='M20.445 6.889c-1.667.555-3.89-1.667-3.334-3.333m-.134.134-3.59 3.59a12.18 12.18 0 0 0-3.204 5.659l-.174.694a.295.295 0 0 0 .358.358l.694-.174a12.18 12.18 0 0 0 5.658-3.203l3.59-3.59a2.357 2.357 0 1 0-3.332-3.334Z'
      stroke='currentColor'
      strokeWidth={1.5}
    />
    <path
      d='M12 3c-1.023 0-2.047.118-3.05.353A7.511 7.511 0 0 0 3.353 8.95a13.354 13.354 0 0 0 0 6.1 7.511 7.511 0 0 0 5.597 5.597c2.006.47 4.094.47 6.1 0a7.511 7.511 0 0 0 5.597-5.597c.235-1.003.353-2.027.353-3.05'
      stroke='currentColor'
      strokeWidth={1.5}
      strokeLinecap='round'
    />
  </svg>
)

export default SvgEdit
