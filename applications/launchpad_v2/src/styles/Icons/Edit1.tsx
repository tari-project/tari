import * as React from 'react'
import { SVGProps } from 'react'

const SvgEdit1 = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-edit1'
    {...props}
  >
    <path
      d='M16.5 9.136c-1.818.606-4.242-1.818-3.636-3.636m.747-.747L9.694 8.67a13.287 13.287 0 0 0-3.495 6.172l-.19.758a.322.322 0 0 0 .391.39l.758-.19a13.286 13.286 0 0 0 6.172-3.494l3.917-3.917a2.571 2.571 0 1 0-3.636-3.636Z'
      stroke='currentColor'
      strokeWidth={1.5}
    />
    <path
      d='M19 20H5'
      stroke='currentColor'
      strokeWidth={1.5}
      strokeLinecap='round'
      strokeLinejoin='round'
    />
  </svg>
)

export default SvgEdit1
